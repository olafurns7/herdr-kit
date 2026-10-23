//! The resident observer only polls completed threads; 90-second starts run elsewhere.
use super::*;
use std::{collections::BTreeMap, sync::Arc, thread::JoinHandle, time::Instant};

pub(crate) struct Scheduler {
    due: Instant,
    scan: Option<JoinHandle<Result<Vec<i64>>>>,
    active: BTreeMap<i64, JoinHandle<Result<Value>>>,
}
impl Default for Scheduler {
    fn default() -> Self {
        Self {
            due: Instant::now(),
            scan: None,
            active: BTreeMap::new(),
        }
    }
}
impl Scheduler {
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
                Ok(Ok(_)) => {}
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
        if self.scan.as_ref().is_some_and(|j| j.is_finished()) {
            match self.scan.take().unwrap().join() {
                Ok(Ok(roots)) => {
                    for root in roots {
                        self.active
                            .entry(root)
                            .or_insert_with(|| std::thread::spawn(move || drive(root)));
                    }
                }
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
        if Instant::now() >= self.due && self.scan.is_none() {
            self.due = Instant::now() + Duration::from_secs(5);
            self.scan = Some(std::thread::spawn(|| {
                let value = taskr(&["work".into(), "campaigns".into()], None)?;
                value["campaigns"]
                    .as_array()
                    .ok_or_else(|| failure("campaigns snapshot missing"))?
                    .iter()
                    .map(|s| number(s, "campaign"))
                    .collect()
            }));
        }
    }
}
