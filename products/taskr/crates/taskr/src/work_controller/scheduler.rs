//! The resident observer only polls completed threads; 90-second starts run elsewhere.
use super::*;
use std::{collections::BTreeMap, sync::Arc, thread::JoinHandle, time::Instant};

fn scan(mut cli: impl FnMut(&[String]) -> Result<Value>) -> Result<Vec<i64>> {
    let value = cli(&["work".into(), "campaigns".into()])?;
    let (rows, truncated) = scan_rows(&value, "campaigns")?;
    if truncated {
        return Err(failure(format!(
            "campaign scan bounded at {} roots; additional enabled campaigns were not observed; drive dispatch withheld",
            value["limit"]
        )));
    }
    rows.iter().map(|s| number(s, "campaign")).collect()
}

pub(crate) struct Scheduler {
    due: Instant,
    reconciliation: Option<JoinHandle<Result<Value>>>,
    reconciled: bool,
    scan: Option<JoinHandle<Result<Vec<i64>>>>,
    active: BTreeMap<i64, JoinHandle<Result<Value>>>,
}
impl Default for Scheduler {
    fn default() -> Self {
        Self {
            due: Instant::now(),
            reconciliation: None,
            reconciled: false,
            scan: None,
            active: BTreeMap::new(),
        }
    }
}
impl Scheduler {
    fn confirmation(&mut self, result: Result<Value>) -> Result<bool> {
        self.reconciled = false;
        if result?["ok"] != true {
            return Err(failure(
                "jobs reconciliation was not confirmed; work dispatch withheld",
            ));
        }
        self.reconciled = true;
        Ok(self.scan.is_none())
    }
    pub(crate) fn tick(&mut self, log: &Arc<crate::daemon::Log>) {
        let finished = self
            .active
            .iter()
            .filter(|(_, job)| job.is_finished())
            .map(|(root, _)| *root)
            .collect::<Vec<_>>();
        for root in finished {
            let result = self.active.remove(&root).unwrap().join();
            match result {
                Ok(Ok(value)) => {
                    if value["bounded_conditions"]
                        .as_array()
                        .is_some_and(|a| !a.is_empty())
                    {
                        log.limited(
                            &format!("work-controller-bound-{root}"),
                            Duration::from_secs(60),
                            &format!(
                                "work controller {root} bounded scan: {}",
                                value["bounded_conditions"]
                            ),
                        );
                    }
                }
                Ok(Err(e)) => log.limited(
                    &format!("work-controller-{root}"),
                    Duration::from_secs(60),
                    &format!("work controller {root}: {}", e.message),
                ),
                Err(_) => log.limited(
                    &format!("work-controller-{root}"),
                    Duration::from_secs(60),
                    "work controller thread panicked; durable assignment requires reconciliation",
                ),
            }
        }
        if self
            .reconciliation
            .as_ref()
            .is_some_and(|j| j.is_finished())
        {
            let result = self
                .reconciliation
                .take()
                .unwrap()
                .join()
                .unwrap_or_else(|_| Err(failure("jobs reconciliation thread panicked")));
            match self.confirmation(result) {
                Ok(true) => self.scan = Some(std::thread::spawn(|| scan(|args| taskr(args, None)))),
                Ok(false) => {}
                Err(e) => log.limited(
                    "jobs-reconcile",
                    Duration::from_secs(60),
                    &format!("jobs reconciliation: {}", e.message),
                ),
            }
        }
        if self.scan.as_ref().is_some_and(|j| j.is_finished()) {
            match self.scan.take().unwrap().join() {
                Ok(Ok(roots)) if self.reconciled => {
                    for root in roots {
                        self.active
                            .entry(root)
                            .or_insert_with(|| std::thread::spawn(move || drive(root)));
                    }
                }
                Ok(Ok(_)) => {}
                Ok(Err(e)) => log.limited(
                    "work-controller-scan",
                    Duration::from_secs(60),
                    &format!("work campaign scan: {}", e.message),
                ),
                Err(_) => log.limited(
                    "work-controller-scan",
                    Duration::from_secs(60),
                    "work campaign scan panicked",
                ),
            }
        }
        if Instant::now() >= self.due && self.reconciliation.is_none() {
            self.due = Instant::now() + Duration::from_secs(5);
            self.reconciliation = Some(std::thread::spawn(|| {
                taskr(&["jobs".into(), "reconcile".into()], None)
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_is_due_and_reconciliation_precedes_campaign_dispatch() {
        let mut scheduler = Scheduler::default();
        assert!(scheduler.due <= Instant::now());
        assert!(scheduler.scan.is_none() && scheduler.active.is_empty() && !scheduler.reconciled);
        assert!(
            scheduler
                .confirmation(Ok(json!({"ok":true,"released":1})))
                .unwrap()
        );
        assert!(scheduler.reconciled);
        let mut calls = Vec::new();
        let roots = scan(|args| {
            calls.push(args.join(" "));
            Ok(json!({"limit":1000,"returned":1,"truncated":false,"campaigns":[{"campaign":7}]}))
        })
        .unwrap();
        assert_eq!(calls, ["work campaigns"]);
        assert_eq!(roots, [7]);
    }
    #[test]
    fn failed_or_unconfirmed_reconciliation_cannot_dispatch_campaigns() {
        for response in [
            Err(failure("unreachable")),
            Ok(json!({"ok":false})),
            Ok(json!({"queued":true})),
        ] {
            let mut scheduler = Scheduler {
                reconciled: true,
                ..Scheduler::default()
            };
            assert!(scheduler.confirmation(response).is_err());
            assert!(
                !scheduler.reconciled && scheduler.scan.is_none() && scheduler.active.is_empty()
            );
        }
    }
    #[test]
    fn slow_campaign_scan_does_not_own_reconciliation_schedule() {
        let (send, recv) = std::sync::mpsc::channel();
        let scan = std::thread::spawn(move || {
            recv.recv_timeout(Duration::from_secs(2))
                .expect("release slow scan fixture");
            Ok(Vec::new())
        });
        let mut scheduler = Scheduler {
            scan: Some(scan),
            ..Scheduler::default()
        };
        assert!(!scheduler.confirmation(Ok(json!({"ok":true}))).unwrap());
        assert!(scheduler.reconciliation.is_none() && scheduler.due <= Instant::now());
        send.send(()).unwrap();
        scheduler.scan.take().unwrap().join().unwrap().unwrap();
    }
    #[test]
    fn truncated_campaign_scan_reports_a_bound_without_dispatching_roots() {
        let error = scan(|_| {
            Ok(json!({"limit":1,"returned":1,"truncated":true,"campaigns":[{"campaign":7}]}))
        })
        .unwrap_err();
        assert!(
            error.message.contains("bounded at 1 roots")
                && error.message.contains("dispatch withheld")
        );
    }
}
