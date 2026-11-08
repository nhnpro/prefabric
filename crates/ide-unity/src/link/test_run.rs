//! Test runs in the Editor through the Test Framework (`tests.run`, `tests/event`, `tests.cancel`),
//! and the console text of their events: a marker line per event for the IDE's test tree, and a
//! readable line per result.

use super::{Inner, UnityLink};
use ide_core::{IdeError, IdeResult};
use serde_json::{Value, json};
use std::time::Duration;

/// Prefix of the console lines that carry a `tests/event` as JSON (`testing/unity.ts` reads them).
pub const MARKER: &str = "##unity-test ";

pub(super) type Listener = Box<dyn FnMut(TestUpdate) + Send>;

pub enum TestUpdate {
    /// A `tests/event` of the run: `runStarted`, `testStarted`, `testFinished` or `runFinished`.
    Event(Value),
    /// The Editor went away (quit, crashed, an incompatible package) before the run finished.
    Lost,
}

/// What a run executes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TestRun {
    /// `editMode` or `playMode`.
    pub mode: String,
    pub assembly: Option<String>,
    /// Full names of tests or fixtures; empty runs all of them.
    pub filter: Vec<String>,
}

impl UnityLink {
    /// Starts a test run; `listener` gets its events until `runFinished` or [`TestUpdate::Lost`].
    /// `id` is the IDE's: events can arrive before the Editor answers. Blocking.
    pub fn run_tests(
        &self,
        id: &str,
        run: &TestRun,
        listener: impl FnMut(TestUpdate) + Send + 'static,
    ) -> IdeResult<()> {
        self.inner.test_runs.lock().insert(id.to_owned(), Box::new(listener));
        let params = json!({"runId": id, "mode": run.mode, "assembly": run.assembly, "filter": run.filter});
        self.request("tests.run", params, Duration::from_secs(30)).map(drop).map_err(|e| {
            self.inner.test_runs.lock().remove(id);
            if e.to_string().contains("unknown method") {
                IdeError::unavailable("Running Unity tests needs the Test Framework package, version 1.3 or later")
            } else {
                e
            }
        })
    }

    /// Asks the Editor to cancel the run; false when it has no such run in progress, which also
    /// forgets the run's listener (nothing will finish it). Blocking.
    pub fn cancel_tests(&self, id: &str) -> bool {
        let cancelled = self
            .request("tests.cancel", json!({"runId": id}), Duration::from_secs(10))
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !cancelled {
            self.inner.test_runs.lock().remove(id);
        }
        cancelled
    }
}

impl Inner {
    pub(super) fn test_event(&self, params: &Value) {
        let Some(id) = params.get("runId").and_then(Value::as_str) else { return };
        let finished = params.get("event").and_then(Value::as_str) == Some("runFinished");
        let mut runs = self.test_runs.lock();
        let Some(listener) = runs.get_mut(id) else { return };
        listener(TestUpdate::Event(params.clone()));
        if finished {
            runs.remove(id);
        }
    }

    pub(super) fn lose_test_runs(&self) {
        let lost: Vec<_> = self.test_runs.lock().drain().collect();
        for (_, mut listener) in lost {
            listener(TestUpdate::Lost);
        }
    }
}

/// Console text of an update: (stdout, stderr).
pub fn console_text(update: &TestUpdate) -> (String, String) {
    let event = match update {
        TestUpdate::Lost => return (String::new(), "The Unity Editor closed before the tests finished.\n".into()),
        TestUpdate::Event(event) => event,
    };
    let str_of = |key: &str| event.get(key).and_then(Value::as_str).unwrap_or_default();
    let num = |key: &str| event.get(key).and_then(Value::as_f64).unwrap_or(0.0);
    let mut out = format!("{MARKER}{event}\n");
    match str_of("event") {
        "testFinished" => {
            let output = str_of("output");
            if !output.is_empty() {
                out.push_str(output);
                if !output.ends_with('\n') {
                    out.push('\n');
                }
            }
            let status = match str_of("status") {
                "passed" => "Passed",
                "failed" => "Failed",
                "skipped" => "Skipped",
                _ => "Inconclusive",
            };
            out.push_str(&format!("{status} {} ({} ms)\n", str_of("fullName"), num("durationMs").round()));
            if status != "Passed" {
                for text in [str_of("message"), str_of("stackTrace")] {
                    let text = text.trim_end();
                    if !text.is_empty() {
                        out.push_str(text);
                        out.push('\n');
                    }
                }
            }
        }
        "runFinished" => out.push_str(&format!(
            "\n{} passed, {} failed, {} skipped ({:.1} s)\n",
            num("passed"),
            num("failed"),
            num("skipped"),
            num("durationMs") / 1000.0
        )),
        _ => {}
    }
    (out, String::new())
}

/// The exit code once the run ended: 1 when a test failed, 0 when none did, none when the Editor
/// went away.
pub fn exit_code(update: &TestUpdate) -> Option<Option<i32>> {
    match update {
        TestUpdate::Lost => Some(None),
        TestUpdate::Event(e) if e.get("event").and_then(Value::as_str) == Some("runFinished") => {
            Some(Some(i32::from(e.get("failed").and_then(Value::as_f64).unwrap_or(0.0) > 0.0)))
        }
        TestUpdate::Event(_) => None,
    }
}
