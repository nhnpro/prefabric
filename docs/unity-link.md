# Unity Editor link — protocol v1

Status: **draft, awaiting review** (plan task E1, `docs/proposals/unity-csharp.md`).

The link lets Prefabric drive a running Unity Editor: play mode, asset refresh, the console,
compile errors, tests and the profiler. The Editor side lives in the `dev.prefabric.unity`
package (`unity/Editor/Link/`); the IDE side in `crates/ide-unity/src/link/`.

## Goals and limits

- Only processes on the same machine, run by the same user, can talk to the Editor.
- The Editor never accepts connections from other machines.
- The link survives domain reloads (script recompiles, entering play mode).
- Nothing here is needed for code insight: without the link the IDE works as before.

## Discovery

When the package loads, the Editor starts a listener and writes
`<project>/Library/Prefabric/link.json`:

```json
{ "protocol": 1, "port": 51234, "token": "9f2c…(32 hex digits)", "pid": 4242, "unityVersion": "6000.6.8f1" }
```

- `port`: a free port on `127.0.0.1`, chosen by the OS.
- `token`: 128 random bits (`RandomNumberGenerator`), hex. New for each Editor session.
- `pid`: the Editor's process id. The IDE ignores a file whose process isn't running.
- The port and token are kept in `SessionState`, so a domain reload listens on the same port
  with the same token and rewrites the same file.
- The file is deleted when the Editor quits. `Library/` is ignored by Unity's standard
  `.gitignore`, so the token is never committed.

The IDE watches `Library/Prefabric/link.json` of an open Unity project and connects when it
appears or changes.

## Transport

- TCP, `127.0.0.1` only (`IPAddress.Loopback`); never `0.0.0.0` or `::`.
- UTF-8 JSON-RPC 2.0 messages, **one per line** (`\n`-terminated, no newlines inside a message).
- Requests go both ways in principle; v1 only has IDE → Editor requests and Editor → IDE
  notifications.
- Up to 4 clients at a time (several IDE windows); more are refused.

## Handshake

The first message on a connection must be the `hello` request; anything else, or a wrong
token, gets an error and the connection is closed. The token is compared in constant time. Until the handshake succeeds a
connection gets 5 s and lines of at most 64 KiB (16 MiB after it), and JSON nested deeper than 64
levels is refused. `link.json` is replaced atomically and readable by its owner only (mode 0600).

```json
→ {"jsonrpc":"2.0","id":1,"method":"hello","params":{"token":"9f2c…","protocol":1,"client":"Prefabric 0.1.38"}}
← {"jsonrpc":"2.0","id":1,"result":{"protocol":1,"unityVersion":"6000.6.8f1","projectPath":"/p/Game","state":{…},"compile":{"messages":[…]}}}
```

`compile` in the result holds the last compile's `messages` (as in `compile/finished`), so an IDE
that connects after a compile, or reconnects after the reload that follows it, still has them.

`protocol` in the result is the version both sides use: the lower of the two. An IDE that
needs a newer version than the Editor offers shows "Update the Prefabric package".

Errors: `-32001` bad token, `-32002` handshake required, `-32003` too many clients.

## Editor state

```json
{ "playMode": "edit" | "playing" | "paused", "compiling": false, "updating": false }
```

`updating` is `EditorApplication.isUpdating` (importing assets).

## Requests (IDE → Editor)

| Method           | Params                                            | Result         | Task |
| ---------------- | ------------------------------------------------- | -------------- | ---- |
| `hello`          | `token`, `protocol`, `client`                     | see above      | E2   |
| `editor.state`   | —                                                 | state          | E3   |
| `editor.play`    | —                                                 | state          | E4   |
| `editor.pause`   | `paused?: bool` (toggle if absent)                | state          | E4   |
| `editor.step`    | —                                                 | state          | E4   |
| `editor.stop`    | —                                                 | state          | E4   |
| `assets.refresh` | —                                                 | `null`         | E4   |
| `asset.ping`     | `path` or `guid`                                  | `bool` (found) | E6   |
| `object.select`  | `path`, `fileId`                                  | `bool`         | E6   |
| `tests.run`      | `runId`, `mode`, `assembly?`, `filter?: string[]` | `null`         | E7   |
| `tests.cancel`   | `runId`                                           | `bool`         | E7   |
| `profiler.*`     | later                                             |                | X7   |

Requests run on the Editor's main thread (queued from the socket thread and run from
`EditorApplication.update`). Unknown methods get `-32601`.

## Notifications (Editor → IDE)

| Method                | Params                                                                                                     | Task |
| --------------------- | ---------------------------------------------------------------------------------------------------------- | ---- |
| `editor/stateChanged` | state                                                                                                      | E3   |
| `console/logs`        | `entries: [{ kind: "log" \| "warning" \| "error" \| "exception" \| "assert", message, stackTrace, time }]` | E5   |
| `compile/finished`    | `messages: [{ file, line, column, severity: "error" \| "warning", message }]`                              | E4   |
| `tests/event`         | `runId`, `event`, test fields (below)                                                                      | E7   |

In `compile/finished`, `file` is relative to the project (`Assets/Scripts/Player.cs`), `line` and
`column` are 1-based, and `message` is the compiler's full text (`Assets/A.cs(3,10): error CS1002: ; expected`).
An empty list means the compile succeeded; the IDE then clears the Editor's diagnostics.

`console/logs` batches entries: at most every 100 ms, and at most 500 entries per message. The
field is `kind` (Unity's `LogType`), `time` is the Editor's local `HH:mm:ss.fff`. While no IDE is
connected the Editor keeps the last 1,000 entries and sends them to the next client that connects.

## Tests (E7)

`tests.run` runs tests through the Test Framework's `TestRunnerApi` (package 1.3 or later; the
package's `Prefabric.Unity.Editor.TestRunner` assembly compiles only then, and without it the method
is unknown). The IDE picks `runId`, so events that arrive before the reply still belong to the run.
`mode` is `"editMode"` or `"playMode"`, `assembly` limits the run to one assembly, and `filter` lists
full names of tests or fixtures (`Game.Tests.PlayerTests`, `Game.Tests.PlayerTests.AddCoinCountsCoins`);
none runs all. One run at a time. A run still open when `tests.run` arrives is one the IDE lost
track of: it is cancelled and finished (`runFinished` with `canceled: true`), and if its job had
started, the request fails with "Stopping the previous test run; try again in a moment" while the job
drains (its late events are dropped until it ends, 15 s at most). Runs started from Unity's Test
Runner window send no events.

`tests/event` `event` values:

- `runStarted`: `count`.
- `testStarted`: `fullName`, `name`, `className`, `methodName` (test cases only, not suites).
- `testFinished`: the same, plus `status` (`passed`, `failed`, `skipped`, `inconclusive`),
  `durationMs`, `message`, `stackTrace`, `output`.
- `runFinished`: `passed`, `failed`, `skipped` (with inconclusive), `durationMs`; `canceled: true`
  after `tests.cancel`.

A Play Mode run reloads the domain: the run in progress and events not yet sent are kept in
`SessionState` and sent once the IDE reconnects. `tests.cancel` cancels the Test Framework's job and
sends `runFinished` (with the counts so far) right away, since a job cancelled before it started
never reports; a job that had started drains as above. It returns false when the Editor has no such
run (a reload lost it); the IDE then ends the run itself.

## Reconnecting

A domain reload or Editor restart closes the socket. The IDE reconnects with backoff
(0.5 s doubling to 5 s), rereading `link.json` each time, and shows "Unity: reconnecting".

## Compatibility

New methods, notifications and optional fields don't change `protocol`. A change that breaks an
existing message bumps it; the Editor then answers older clients with the older behaviour, or
refuses them with `-32004`.
