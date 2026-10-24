//! Client file capture and the local/RPC-shared work command parser.
use std::io::Read;
use taskr_core::{
    ExitCode,
    goflag::FlagSet,
    store::{self, work as api},
};

const VERBS: &[&str] = &[
    "configure",
    "add",
    "list",
    "show",
    "revise",
    "queue",
    "claim",
    "reserve",
    "bind",
    "started",
    "observe",
    "uncertain",
    "reconcile",
    "submit",
    "advance",
    "cancel",
    "recover",
    "campaigns",
    "dispatch",
    "inputs",
    "hash",
    "stopped",
    "block",
];
fn flags(json: bool, verb: &str) -> store::Result<FlagSet> {
    if !VERBS.contains(&verb) {
        return Err(store::usage("unknown work verb"));
    }
    let mut f = FlagSet::new("work", json);
    if !matches!(
        verb,
        "list" | "show" | "inputs" | "campaigns" | "dispatch" | "hash" | "submit"
    ) {
        f.int("as", 0, "campaign root")
            .int("generation", -1, "explicit current root generation");
    }
    if !matches!(
        verb,
        "list" | "show" | "inputs" | "campaigns" | "dispatch" | "advance" | "recover"
    ) {
        f.string("data", "", "bounded typed JSON").string(
            "file",
            "",
            "UTF-8 JSON file captured on client",
        );
    }
    if verb == "dispatch" {
        f.int("limit", 32, "thin nonterminal rows, 1..64");
    }
    if verb == "configure" {
        f.string("enabled", "off", "on or off")
            .int("max-active", 1, "1..64")
            .int("max-review-pending", 1, "1..64")
            .string("workspace", "", "explicit campaign task workspace")
            .bool(
                "no-slotr",
                false,
                "explicit host heavy-check admission exception",
            );
    }
    Ok(f)
}
fn parse(json: bool, args: &[String]) -> store::Result<(String, FlagSet)> {
    let verb = args
        .first()
        .ok_or_else(|| store::usage("taskr work VERB [ID] --data JSON"))?;
    let mut f = flags(json, verb)?;
    // Unlike Go's legacy parser, work rejects repeat flags, including local invocations.
    let mut seen = std::collections::BTreeSet::new();
    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" {
            break;
        }
        if arg.starts_with('-') && arg != "-" {
            let name = arg.trim_start_matches('-').split('=').next().unwrap();
            if !seen.insert(name) {
                return Err(store::usage(format!("repeated work flag --{name}")));
            }
            if !arg.contains('=') && !matches!(name, "json" | "help" | "h" | "no-slotr") {
                i += 1;
            }
        }
        i += 1;
    }
    let n = usize::from(!matches!(verb.as_str(), "add" | "campaigns" | "hash"));
    f.parse(&args[1..], n, n).map_err(store::usage)?;
    if f.was_set("data") && f.was_set("file") {
        return Err(store::usage("use --data or --file, never both"));
    }
    Ok((verb.clone(), f))
}
fn file(path: &str) -> store::Result<String> {
    let input = std::fs::File::open(path).map_err(|e| store::usage(format!("work --file: {e}")))?;
    let mut bytes = Vec::new();
    input
        .take((api::MAX_JSON + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| store::usage(format!("work --file: {e}")))?;
    if bytes.len() > api::MAX_JSON {
        return Err(store::usage("work JSON exceeds 256 KiB"));
    }
    let raw = String::from_utf8(bytes).map_err(|_| store::usage("work file must be UTF-8 JSON"))?;
    api::parse::<serde_json::Value>(&raw)?;
    Ok(raw)
}
/// Called by the client router before constructing RPC argv. The server never reads a path.
pub(crate) fn inline_files(args: &mut Vec<String>) -> store::Result<()> {
    let (_, f) = parse(false, args)?;
    if !f.was_set("file") {
        return Ok(());
    }
    let raw = file(f.get_string("file"))?;
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--" {
            break;
        }
        let name = args[i].trim_start_matches('-').split('=').next().unwrap();
        if args[i].starts_with('-') && name == "file" {
            if args[i].contains('=') {
                args.splice(i..=i, ["--data".into(), raw]);
            } else {
                args.splice(i..=i + 1, ["--data".into(), raw]);
            }
            return Ok(());
        }
        if args[i].starts_with('-')
            && !args[i].contains('=')
            && !matches!(name, "json" | "help" | "h" | "no-slotr")
        {
            i += 1;
        }
        i += 1;
    }
    Err(store::usage("missing file argument"))
}
fn raw(f: &FlagSet) -> store::Result<String> {
    if f.was_set("file") {
        if store::rpc_context().is_some() {
            return Err(store::usage(
                "RPC work --file must be captured by client as --data",
            ));
        }
        file(f.get_string("file"))
    } else {
        let raw = f.get_string("data");
        api::parse::<serde_json::Value>(raw)?;
        Ok(raw.into())
    }
}
fn invoke(
    db: &mut taskr_core::db::Connection,
    verb: &str,
    f: &FlagSet,
) -> store::Result<serde_json::Value> {
    let id = if matches!(verb, "add" | "campaigns" | "hash") {
        0
    } else {
        store::id(&f.positional[0], "work/run/launch/campaign ID")?
    };
    let a = if matches!(
        verb,
        "list" | "show" | "inputs" | "campaigns" | "dispatch" | "hash" | "submit"
    ) {
        api::Authority {
            root: 0,
            generation: -1,
        }
    } else {
        if f.get_int("as") <= 0 || f.get_int("generation") < 0 || !f.was_set("generation") {
            return Err(store::usage(
                "work mutation requires --as ROOT --generation GEN",
            ));
        }
        api::Authority {
            root: f.get_int("as"),
            generation: f.get_int("generation"),
        }
    };
    match verb {
        "configure" => {
            let has_flags = [
                "enabled",
                "max-active",
                "max-review-pending",
                "workspace",
                "no-slotr",
            ]
            .iter()
            .any(|n| f.was_set(n));
            let input = if f.was_set("data") || f.was_set("file") {
                if has_flags {
                    return Err(store::usage("configure: use JSON or configuration flags"));
                }
                api::parse(&raw(f)?)?
            } else {
                let enabled = match f.get_string("enabled") {
                    "on" => true,
                    "off" => false,
                    _ => return Err(store::usage("enabled must be on or off")),
                };
                api::Configure {
                    enabled,
                    workspace: (!f.get_string("workspace").is_empty())
                        .then(|| f.get_string("workspace").to_string()),
                    no_slotr: f.get_bool("no-slotr"),
                    max_active: u32::try_from(f.get_int("max-active"))
                        .map_err(|_| store::usage("invalid max-active"))?,
                    max_review_pending: u32::try_from(f.get_int("max-review-pending"))
                        .map_err(|_| store::usage("invalid max-review-pending"))?,
                }
            };
            api::configure(db, a, id, input)
        }
        "add" => api::add(db, a, api::parse(&raw(f)?)?),
        "campaigns" => api::campaigns(db),
        "dispatch" => api::dispatch(
            db,
            id,
            u32::try_from(f.get_int("limit"))
                .map_err(|_| store::usage("dispatch limit must be 1..64"))?,
        ),
        "inputs" => api::inputs(db, id),
        "hash" => {
            let mut input: api::HashInput = api::parse(&raw(f)?)?;
            input.profile.digest = api::profile_digest(&input.profile)?;
            Ok(
                serde_json::json!({"profile":input.profile,"profile_digest":input.profile.digest,"input_digest":api::input_digest(&input.profile,&input.dependencies)?}),
            )
        }
        "list" => api::list(db, id),
        "show" => api::show(db, id),
        "revise" => api::revise(db, a, id, api::parse(&raw(f)?)?),
        "queue" => api::queue(db, a, id, api::parse(&raw(f)?)?),
        "claim" => api::claim(db, a, id, api::parse(&raw(f)?)?),
        "reserve" => api::reserve(db, a, id, api::parse(&raw(f)?)?),
        "bind" => api::bind(db, a, id, api::parse(&raw(f)?)?),
        "observe" => api::observe(db, a, id, api::parse(&raw(f)?)?),
        "started" => {
            if f.was_set("data") || f.was_set("file") {
                api::started_observed(db, a, id, api::parse(&raw(f)?)?)
            } else {
                api::started(db, a, id)
            }
        }
        "stopped" => api::stopped(db, a, id, api::parse(&raw(f)?)?),
        "block" => api::block(db, a, id, api::parse(&raw(f)?)?),
        "uncertain" => api::uncertain(db, a, id, api::parse(&raw(f)?)?),
        "reconcile" => api::reconcile(db, a, id, api::parse(&raw(f)?)?),
        "submit" => api::submit(db, id, api::parse(&raw(f)?)?),
        "advance" => api::advance(db, a, id),
        "cancel" => api::cancel(db, a, id, api::parse(&raw(f)?)?),
        "recover" => api::recover(db, a, id),
        _ => Err(store::usage("unknown work verb")),
    }
}
pub fn dispatch(json: bool, args: &[String]) -> Option<ExitCode> {
    if args.first()? != "work" {
        return None;
    }
    let run = || -> store::Result<(bool, serde_json::Value)> {
        if args.len() == 1
            || args
                .get(1)
                .is_some_and(|a| matches!(a.as_str(), "--help" | "-h"))
        {
            println!(
                "taskr work <{}> [ID] [--as ROOT --generation GEN] [--data JSON | --file PATH]",
                VERBS.join("|")
            );
            return Ok((json, serde_json::Value::Null));
        }
        let (verb, f) = parse(json, &args[1..])?;
        if f.help() {
            print!(
                "{}",
                f.usage(
                    "taskr work VERB [ID] [--as ROOT --generation GEN] [--data JSON | --file PATH]"
                )
            );
            return Ok((f.json(), serde_json::Value::Null));
        }
        let mut db = taskr_core::db::path()
            .and_then(|p| taskr_core::db::open(&p))
            .map_err(|e| store::Error {
                code: ExitCode::Database,
                message: e,
            })?;
        Ok((f.json(), invoke(&mut db, &verb, &f)?))
    };
    Some(match run() {
        Ok((json, v)) => {
            if !v.is_null() {
                crate::cli::emit(json, &v, false);
            }
            ExitCode::Ok
        }
        Err(e) => crate::cli::error(
            json || args.iter().any(|a| a == "--json"),
            "work",
            &e.message,
            e.code,
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_repeat_and_conflicting_flags_fail_before_rpc() {
        for args in [
            vec!["add", "--mystery", "x"],
            vec!["add", "--as", "1", "--as=1"],
            vec!["add", "--data", "{}", "--file", "x"],
        ] {
            assert!(
                parse(
                    false,
                    &args.into_iter().map(String::from).collect::<Vec<_>>()
                )
                .is_err()
            );
        }
    }
    #[test]
    fn client_file_is_inline_even_when_server_cannot_read_path() {
        let path =
            std::env::temp_dir().join(format!("taskr-work-file-{}.json", std::process::id()));
        std::fs::write(&path, r#"{"campaign":1,"title":"backlog"}"#).unwrap();
        let mut args = vec![
            "add".into(),
            "--file".into(),
            path.to_string_lossy().into_owned(),
        ];
        inline_files(&mut args).unwrap();
        std::fs::remove_file(&path).unwrap();
        let (_, f) = parse(false, &args).unwrap();
        assert!(!f.was_set("file"));
        assert_eq!(
            api::parse::<api::Add>(f.get_string("data")).unwrap().title,
            "backlog"
        );
        assert!(file(path.to_str().unwrap()).is_err());
        assert!(api::parse::<api::Add>("{").is_err());
    }
}
