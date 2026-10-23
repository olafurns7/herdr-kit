//! Jobs commands run through taskr's normal local/RPC dispatch, never a second ledger.
use serde::Deserialize;
use taskr_core::{
    ExitCode,
    goflag::FlagSet,
    store::{self, jobs as api},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Work {
    work: i64,
}
fn empty(raw: &str) -> store::Result<()> {
    let value: std::collections::BTreeMap<String, serde_json::Value> = store::work::parse(raw)?;
    if !value.is_empty() {
        return Err(store::usage("this jobs verb accepts no data fields"));
    }
    Ok(())
}
fn invoke(
    db: &mut taskr_core::db::Connection,
    verb: &str,
    f: &FlagSet,
) -> store::Result<serde_json::Value> {
    let raw = f.get_string("data");
    // Validate size and JSON even for maintenance/no-argument commands.
    store::work::parse::<serde_json::Value>(raw)?;
    if verb == "reconcile" {
        empty(raw)?;
        return api::reconcile(db);
    }
    if !f.was_set("generation") {
        return Err(store::usage("jobs requires --generation GEN"));
    }
    let s = api::Session {
        root: f.get_int("as"),
        generation: f.get_int("generation"),
        binding: f.get_string("binding").into(),
    };
    let handle = || store::work::parse::<api::Handle>(raw).map(|v| v.handle);
    match verb {
        "validate" => {
            empty(raw)?;
            api::validate(db, &s)
        }
        "get" => api::get_work(db, &s, store::work::parse::<Work>(raw)?.work),
        "add" => api::add(db, &s, store::work::parse(raw)?),
        "await" => api::await_work(db, &s, store::work::parse(raw)?),
        "recover" => {
            empty(raw)?;
            api::recover(db, &s)
        }
        "poll" => api::poll(db, &s, &handle()?),
        "release" => api::release(db, &s, &handle()?, false),
        "cancel" => api::release(db, &s, &handle()?, true),
        "receipt" => api::receipt(db, &s, store::work::parse(raw)?),
        "allow-ask" => api::allow_ask(db, &s, store::work::parse::<api::Ask>(raw)?.ask),
        "owner" => api::owner_input(db, &s, store::work::parse::<api::Ask>(raw)?.ask),
        "update" => api::update(db, &s, store::work::parse(raw)?),
        _ => Err(store::usage("unknown jobs verb")),
    }
}
pub fn dispatch(json: bool, args: &[String]) -> Option<ExitCode> {
    if args.first()?.as_str() != "jobs" {
        return None;
    }
    let execute = || -> store::Result<()> {
        let Some(verb) = args.get(1) else {
            return Err(store::usage(
                "taskr jobs VERB --as ROOT --generation GEN --binding TOKEN [--data JSON]",
            ));
        };
        if ![
            "validate",
            "get",
            "add",
            "await",
            "recover",
            "poll",
            "release",
            "cancel",
            "receipt",
            "allow-ask",
            "owner",
            "update",
            "reconcile",
        ]
        .contains(&verb.as_str())
        {
            return Err(store::usage("unknown jobs verb"));
        }
        if let Some(flag) = crate::write::repeated(
            &args[2..],
            &["as", "generation", "binding", "data", "json", "help"],
        ) {
            return Err(store::usage(format!("repeated jobs flag --{flag}")));
        }
        let mut f = FlagSet::new("jobs", json);
        f.int("as", 0, "campaign root")
            .int("generation", -1, "current root generation")
            .string("binding", "", "fixed native session binding identity")
            .string("data", "{}", "bounded typed JSON; no server file reads");
        f.parse(&args[2..], 0, 0).map_err(store::usage)?;
        if f.help() {
            print!(
                "{}",
                f.usage("taskr jobs VERB --as ROOT --generation GEN --binding TOKEN [--data JSON]")
            );
            return Ok(());
        }
        let mut db = taskr_core::db::path()
            .and_then(|p| taskr_core::db::open(&p))
            .map_err(|message| store::Error {
                code: ExitCode::Database,
                message,
            })?;
        let value = invoke(&mut db, verb, &f)?;
        crate::cli::emit(f.json(), &value, false);
        Ok(())
    };
    Some(match execute() {
        Ok(()) => ExitCode::Ok,
        Err(e) => crate::cli::error(
            json || args.iter().any(|a| a == "--json"),
            "jobs",
            &e.message,
            e.code,
        ),
    })
}
