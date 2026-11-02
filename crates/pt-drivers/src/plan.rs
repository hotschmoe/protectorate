use crate::error::DriverError;
use crate::event::Harness;
use crate::protocol::Protocol;
use crate::spec::{Permission, SpawnSpec};
use crate::{claude, codex, grok, pi};

pub(crate) type UuidSource<'a> = &'a mut dyn FnMut() -> std::io::Result<String>;

/// The effort levels a harness's own CLI accepts. Codex's depend on the model, so its driver
/// checks them against Codex's catalog instead.
fn effort_levels(harness: Harness) -> Option<&'static [&'static str]> {
    match harness {
        Harness::Claude => Some(&["low", "medium", "high", "xhigh", "max"]),
        Harness::Grok => Some(&["low", "medium", "high", "xhigh"]),
        Harness::Pi => Some(&["off", "minimal", "low", "medium", "high", "xhigh", "max"]),
        Harness::Codex => None,
    }
}

/// Drops an effort the harness would refuse and says so, so the owner is told rather than the
/// launch failing or the CLI quietly choosing for itself.
pub(crate) fn vet_effort(spec: &mut SpawnSpec) -> Option<String> {
    let effort = spec.effort.as_deref()?;
    let levels = effort_levels(spec.harness)?;
    if levels.contains(&effort) {
        return None;
    }
    let note = format!(
        "{}: effort {effort:?} is not one of {}; using the default",
        spec.harness,
        levels.join(", ")
    );
    spec.effort = None;
    Some(note)
}

/// The harness argv after the program, and the protocol state for `spec`; `uuid` yields
/// fresh random UUIDs.
pub(crate) fn plan(
    spec: &SpawnSpec,
    uuid: UuidSource<'_>,
) -> Result<(Vec<String>, Box<dyn Protocol>), DriverError> {
    let cwd = spec
        .cwd
        .to_str()
        .ok_or(DriverError::Invalid("cwd must be UTF-8"))?
        .to_owned();
    Ok(match spec.harness {
        Harness::Claude => {
            let id = match &spec.resume {
                Some(id) => id.clone(),
                None => random(uuid)?,
            };
            let p = claude::Claude::new(id, random(uuid)?, spec, cwd);
            (p.argv(spec), Box::new(p))
        }
        Harness::Codex => {
            let p = codex::Codex::new(spec, cwd);
            (p.argv(spec), Box::new(p))
        }
        Harness::Grok => {
            let p = grok::Grok::new(spec, cwd);
            (p.argv(spec), Box::new(p))
        }
        Harness::Pi => {
            if spec.permission == Permission::Supervised {
                return Err(DriverError::Unsupported("Pi has no approval gate"));
            }
            let session_id = match &spec.resume {
                Some(id) => id.clone(),
                None => random(uuid)?,
            };
            let p = pi::Pi::new(spec, session_id);
            (p.argv(spec), Box::new(p))
        }
    })
}

fn random(uuid: UuidSource<'_>) -> Result<String, DriverError> {
    uuid().map_err(|source| DriverError::Spawn {
        program: "/dev/urandom".into(),
        source,
    })
}
