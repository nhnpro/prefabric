using System;
using System.Collections.Generic;
using System.Linq;
using UnityEditor;
using UnityEditor.TestTools.TestRunner.Api;
using UnityEngine;
using EditorLink = Prefabric.Unity.Editor.Link.Link;
using Json = Prefabric.Unity.Editor.Link.Json;

namespace Prefabric.Unity.Editor.TestRunner
{
    /// <summary>
    /// Runs the project's tests for the IDE through <c>TestRunnerApi</c> (<c>tests.run</c>, <c>tests.cancel</c>)
    /// and reports them as <c>tests/event</c>. Compiled only with the Test Framework package (1.3 or later).
    /// A Play Mode run reloads the domain: the run in progress and unsent events live in <c>SessionState</c>.
    /// </summary>
    [InitializeOnLoad]
    internal static class LinkTests
    {
        const string RunKey = "Prefabric.Tests.Run";
        const string GuidKey = "Prefabric.Tests.Guid";
        const string PendingKey = "Prefabric.Tests.Pending";
        const string StartKey = "Prefabric.Tests.Start";
        /// <summary>The run's job reported RunStarted (1).</summary>
        const string StartedKey = "Prefabric.Tests.Started";
        /// <summary>When a cancelled job that had started was let go: its late callbacks are dropped
        /// until its RunFinished, or for <see cref="DrainSeconds"/> at most.</summary>
        const string DrainKey = "Prefabric.Tests.Drain";
        const double DrainSeconds = 15;
        static readonly string[] CountKeys = { "passed", "failed", "skipped" };

        static readonly List<object> pending = new List<object>();
        static readonly TestRunnerApi api;

        static LinkTests()
        {
            if (!EditorLink.Enabled)
                return;
            api = ScriptableObject.CreateInstance<TestRunnerApi>();
            api.RegisterCallbacks(new Callbacks());
            EditorLink.Register("tests.run", Run);
            EditorLink.Register("tests.cancel", Cancel);
            if (Json.Parse(SessionState.GetString(PendingKey, "[]")) is List<object> saved)
                pending.AddRange(saved);
            SessionState.EraseString(PendingKey);
            EditorApplication.update += Flush;
            AssemblyReloadEvents.beforeAssemblyReload += () =>
            {
                Flush();
                if (pending.Count > 0)
                    SessionState.SetString(PendingKey, Json.Write(pending));
            };
        }

        /// <summary><c>runId</c> (the IDE's), <c>mode</c>, <c>assembly?</c>, <c>filter?</c> (full names of tests or fixtures).</summary>
        static object Run(Dictionary<string, object> p)
        {
            var runId = p.TryGetValue("runId", out var r) ? r as string : null;
            if (string.IsNullOrEmpty(runId))
                throw new ArgumentException("runId is required");
            if (Draining())
                throw new InvalidOperationException("The previous test run is still stopping; try again in a moment");
            // A run still open here is one the IDE lost track of (a restart, a reload during Stop):
            // this run replaces it.
            if (SessionState.GetString(RunKey, "").Length > 0 && Stop())
                throw new InvalidOperationException("Stopping the previous test run; try again in a moment");
            var filter = new Filter
            {
                testMode = p.TryGetValue("mode", out var m) && m as string == "playMode" ? TestMode.PlayMode : TestMode.EditMode,
            };
            if (p.TryGetValue("assembly", out var a) && a is string assembly && assembly.Length > 0)
                filter.assemblyNames = new[] { assembly };
            if (p.TryGetValue("filter", out var f) && f is List<object> names)
            {
                var tests = names.OfType<string>().Where(n => n.Length > 0).ToArray();
                if (tests.Length > 0)
                    filter.testNames = tests;
            }
            // Before Execute: RunStarted may come before it returns.
            SessionState.SetString(RunKey, runId);
            SessionState.SetFloat(StartKey, (float)EditorApplication.timeSinceStartup);
            SessionState.EraseInt(StartedKey);
            foreach (var key in CountKeys)
                SessionState.EraseInt(CountKey(key));
            try
            {
                SessionState.SetString(GuidKey, api.Execute(new ExecutionSettings(filter)) ?? "");
            }
            catch
            {
                SessionState.EraseString(RunKey);
                throw;
            }
            return null;
        }

        static object Cancel(Dictionary<string, object> p)
        {
            var runId = p.TryGetValue("runId", out var r) ? r as string : null;
            if (string.IsNullOrEmpty(runId) || SessionState.GetString(RunKey, "") != runId)
                return false;
            Stop();
            return true;
        }

        /// <summary>Cancels the open run's job and finishes the run for the IDE: a job cancelled
        /// before it started never reports RunFinished. Returns whether the job had started, in which
        /// case its late callbacks are drained.</summary>
        static bool Stop()
        {
            var guid = SessionState.GetString(GuidKey, "");
            if (guid.Length > 0)
                TestRunnerApi.CancelTestRun(guid);
            var finished = Counts();
            finished["canceled"] = true;
            Send("runFinished", finished);
            var started = SessionState.GetInt(StartedKey, 0) == 1;
            Finished();
            if (started)
                SessionState.SetFloat(DrainKey, (float)EditorApplication.timeSinceStartup);
            return started;
        }

        static bool Draining()
        {
            var since = SessionState.GetFloat(DrainKey, -1);
            if (since < 0)
                return false;
            if (EditorApplication.timeSinceStartup - since < DrainSeconds)
                return true;
            SessionState.EraseFloat(DrainKey);
            return false;
        }

        static string CountKey(string name) => "Prefabric.Tests.Count." + name;

        static Dictionary<string, object> Counts()
        {
            var counts = new Dictionary<string, object>();
            foreach (var key in CountKeys)
                counts[key] = SessionState.GetInt(CountKey(key), 0);
            counts["durationMs"] = (EditorApplication.timeSinceStartup - SessionState.GetFloat(StartKey, 0)) * 1000;
            return counts;
        }

        static void Finished()
        {
            SessionState.EraseString(RunKey);
            SessionState.EraseString(GuidKey);
            SessionState.EraseInt(StartedKey);
        }

        static void Send(string kind, Dictionary<string, object> fields)
        {
            // Runs started from Unity's own Test Runner window aren't the IDE's.
            var runId = SessionState.GetString(RunKey, "");
            if (runId.Length == 0 || Draining())
                return;
            fields["runId"] = runId;
            fields["event"] = kind;
            pending.Add(fields);
            Flush();
        }

        static void Flush()
        {
            if (pending.Count == 0 || !EditorLink.Connected)
                return;
            foreach (var e in pending)
                EditorLink.Notify("tests/event", e);
            pending.Clear();
        }

        static Dictionary<string, object> Fields(ITestAdaptor test) => new Dictionary<string, object>
        {
            ["fullName"] = test.FullName,
            ["name"] = test.Name,
            ["className"] = test.TypeInfo?.FullName ?? "",
            ["methodName"] = test.Method?.Name ?? test.Name,
        };

        sealed class Callbacks : ICallbacks
        {
            public void RunStarted(ITestAdaptor testsToRun)
            {
                if (Draining())
                    return;
                SessionState.SetInt(StartedKey, 1);
                Send("runStarted", new Dictionary<string, object> { ["count"] = testsToRun.TestCaseCount });
            }

            public void RunFinished(ITestResultAdaptor result)
            {
                // The end of a cancelled job: the run it belonged to is finished already.
                if (Draining())
                {
                    SessionState.EraseFloat(DrainKey);
                    return;
                }
                Send("runFinished", new Dictionary<string, object>
                {
                    ["passed"] = result.PassCount,
                    ["failed"] = result.FailCount,
                    ["skipped"] = result.SkipCount + result.InconclusiveCount,
                    ["durationMs"] = result.Duration * 1000,
                });
                Finished();
            }

            public void TestStarted(ITestAdaptor test)
            {
                if (!test.IsSuite)
                    Send("testStarted", Fields(test));
            }

            public void TestFinished(ITestResultAdaptor result)
            {
                if (result.Test.IsSuite)
                    return;
                var fields = Fields(result.Test);
                fields["status"] = result.TestStatus switch
                {
                    TestStatus.Passed => "passed",
                    TestStatus.Failed => "failed",
                    TestStatus.Skipped => "skipped",
                    _ => "inconclusive",
                };
                fields["durationMs"] = result.Duration * 1000;
                fields["message"] = result.Message ?? "";
                fields["stackTrace"] = result.StackTrace ?? "";
                fields["output"] = result.Output ?? "";
                var count = CountKey(result.TestStatus == TestStatus.Passed ? "passed" : result.TestStatus == TestStatus.Failed ? "failed" : "skipped");
                SessionState.SetInt(count, SessionState.GetInt(count, 0) + 1);
                Send("testFinished", fields);
            }
        }
    }
}
