use crate::{
    PriorityCommand,
    config::{self, Config},
    manager, state,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub version: u32,
    pub generation: u64,
    pub campaigns: BTreeMap<String, u32>,
}
pub fn path() -> PathBuf {
    config::xdg("XDG_STATE_HOME", ".local/state").join("slotr/priority.json")
}
pub fn campaign(name: &str) -> Result<()> {
    ensure!(
        !name.trim().is_empty() && !name.chars().any(|c| c.is_control() || "*?[]".contains(c)),
        "campaign must be a nonempty exact name without controls or wildcards"
    );
    Ok(())
}
pub fn read() -> Result<Policy> {
    let read = || -> Result<Policy> {
        let Some(bytes) = state::read_sidecar(&path())? else {
            return Ok(Policy {
                version: 1,
                ..Policy::default()
            });
        };
        let policy: Policy = serde_json::from_slice(&bytes)?;
        ensure!(policy.version == 1, "unsupported policy version");
        for name in policy.campaigns.keys() {
            campaign(name)?;
        }
        Ok(policy)
    };
    read().context("priority_policy_error")
}
impl Policy {
    pub fn weight(&self, cfg: &Config, name: &str, task: &str) -> u32 {
        self.campaigns
            .get(name)
            .copied()
            .unwrap_or_else(|| cfg.priority.level(name, task))
    }
    pub fn source(&self, name: &str, weight: u32) -> &'static str {
        if self.campaigns.contains_key(name) {
            "managed"
        } else if weight > 0 {
            "legacy"
        } else {
            "default"
        }
    }
    pub fn refresh(&self, s: &mut state::State, cfg: &Config) {
        let legacy = cfg.priority.read();
        let weight = |campaign: &str, task: &str| {
            self.campaigns
                .get(campaign)
                .copied()
                .unwrap_or_else(|| cfg.priority.level_from(campaign, task, legacy.as_deref()))
        };
        for activity in &mut s.scheduling.activity {
            activity.weight = weight(&activity.campaign, &activity.task);
        }
        for q in s
            .queue
            .iter_mut()
            .chain(s.holders.iter_mut().map(|h| &mut h.request))
        {
            q.level = weight(&q.campaign, &q.task);
            q.policy_generation = self.generation;
        }
    }
    pub fn status(&self, cfg: &Config) -> serde_json::Value {
        let mut data = cfg.priority.status();
        data.as_object_mut().unwrap().extend(json!({"path":path(),"generation":self.generation,"campaigns":self.campaigns,"error":null,"legacy":cfg.priority.status()}).as_object().unwrap().clone());
        data
    }
}
pub fn command(command: PriorityCommand, cfg: &Config) -> Result<i32> {
    match command {
        PriorityCommand::List { json: as_json } => {
            let data = match read() {
                Ok(policy) => policy.status(cfg),
                Err(e) => json!({"path":path(),"error":format!("{e:#}")}),
            };
            if as_json {
                println!("{data}");
            } else {
                println!("Priority: {data}");
            }
        }
        change => {
            let (name, value) = match change {
                PriorityCommand::Set { campaign, weight } => (campaign, Some(weight)),
                PriorityCommand::Clear { campaign } => (campaign, None),
                _ => unreachable!(),
            };
            campaign(&name)?;
            let (generation, effective) = state::transaction(|s| {
                let mut policy = read()?;
                let old = policy.weight(cfg, &name, "");
                if let Some(value) = value {
                    policy.campaigns.insert(name.clone(), value);
                } else {
                    policy.campaigns.remove(&name);
                }
                policy.generation = policy
                    .generation
                    .checked_add(1)
                    .context("policy generation exhausted")?;
                ensure!(
                    serde_json::to_vec(&policy)?.len() <= 1024 * 1024,
                    "managed policy exceeds 1 MiB"
                );
                state::atomic_json(&path(), &policy)?;
                policy.refresh(s, cfg);
                let new = policy.weight(cfg, &name, "");
                manager::event(json!({"event":"priority_change","campaign":name,"old_weight":old,"weight":new,"override":value,"generation":policy.generation,"caller":manager::caller("cli")}))
                    .context("policy changed but priority_change audit append failed")?;
                Ok((policy.generation, new))
            })?;
            println!("campaign={name} weight={effective} generation={generation}");
        }
    }
    Ok(0)
}
