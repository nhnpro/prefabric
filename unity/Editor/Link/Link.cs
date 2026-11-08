using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.IO;
using System.Threading;
using UnityEditor;
using UnityEditor.Compilation;
using UnityEngine;

namespace Prefabric.Unity.Editor.Link
{
    /// <summary>
    /// Starts the link in the Editor and keeps it across domain reloads: the port and token live in
    /// <c>SessionState</c>, and <c>Library/Prefabric/link.json</c> tells the IDE where to connect.
    /// </summary>
    [InitializeOnLoad]
    internal static class Link
    {
        const string PortKey = "Prefabric.Link.Port";
        const string TokenKey = "Prefabric.Link.Token";
        const string CompileKey = "Prefabric.Link.Compile";

        static readonly LinkServer server;
        static readonly Dictionary<string, LinkServer.Handler> handlers = Handlers();
        static readonly string linkFile;
        static Dictionary<string, object> lastState;

        static Link()
        {
            // Asset import workers load editor code too; batch runs (builds, CI) aren't driven by the IDE.
            if (AssetDatabase.IsAssetImportWorkerProcess() || (Application.isBatchMode && !HasArg("-prefabricLink")))
                return;

            var token = SessionState.GetString(TokenKey, "");
            if (token.Length != 32)
            {
                token = LinkServer.NewToken();
                SessionState.SetString(TokenKey, token);
            }
            try
            {
                server = new LinkServer(SessionState.GetInt(PortKey, 0), token, handlers, Hello);
            }
            catch (Exception e)
            {
                Debug.LogWarning($"Prefabric: the Editor link could not start: {e.Message}");
                return;
            }
            SessionState.SetInt(PortKey, server.Port);

            var project = Directory.GetParent(Application.dataPath)!.FullName;
            linkFile = Path.Combine(project, "Library", "Prefabric", "link.json");
            WriteLinkFile(token);

            EditorApplication.update += Update;
            Application.logMessageReceivedThreaded += OnLog;
            AssemblyReloadEvents.beforeAssemblyReload += () =>
            {
                Application.logMessageReceivedThreaded -= OnLog;
                FlushLogs(int.MaxValue);
                server.Dispose();
            };
            EditorApplication.quitting += () =>
            {
                server.Dispose();
                try
                {
                    File.Delete(linkFile);
                }
                catch (IOException)
                {
                }
            };
            EditorApplication.playModeStateChanged += _ => PublishState();
            EditorApplication.pauseStateChanged += _ => PublishState();
            CompilationPipeline.compilationStarted += _ =>
            {
                compiling.Clear();
                PublishState();
            };
            CompilationPipeline.assemblyCompilationFinished += (_, messages) => Collect(messages);
            CompilationPipeline.compilationFinished += _ => CompileFinished();
            PublishState();
        }

        /// <summary>Whether the link runs in this Editor (not in batch runs or import workers).</summary>
        internal static bool Enabled => server != null;

        /// <summary>Whether an IDE completed the handshake, so notifications reach it.</summary>
        internal static bool Connected => server != null && server.HasReadyClient;

        /// <summary>Adds a request handler from another assembly of the package (the test runner). Main thread.</summary>
        internal static void Register(string method, LinkServer.Handler handler)
        {
            if (server != null)
                lock (handlers)
                    handlers[method] = handler;
        }

        internal static void Notify(string method, object parameters) => server?.Notify(method, parameters);

        static bool HasArg(string name) => Array.IndexOf(Environment.GetCommandLineArgs(), name) >= 0;

        static void WriteLinkFile(string token)
        {
            Directory.CreateDirectory(Path.GetDirectoryName(linkFile)!);
            var json = Json.Write(new Dictionary<string, object>
            {
                ["protocol"] = LinkServer.Protocol,
                ["port"] = server.Port,
                ["token"] = token,
                ["pid"] = System.Diagnostics.Process.GetCurrentProcess().Id,
                ["unityVersion"] = Application.unityVersion,
            });
            // Write then rename over the old file, so the IDE never reads half a file or finds none.
            var temp = linkFile + ".tmp";
            File.WriteAllText(temp, json);
            OwnerOnly(temp);
            if (File.Exists(linkFile))
                File.Replace(temp, linkFile, null);
            else
                File.Move(temp, linkFile);
        }

        [System.Runtime.InteropServices.DllImport("libc", SetLastError = true)]
        static extern int chmod(string path, int mode);

        /// <summary>The token is for this user only (mode 0600; Windows keeps the profile's ACLs).</summary>
        static void OwnerOnly(string path)
        {
            if (Application.platform == RuntimePlatform.WindowsEditor)
                return;
            try
            {
                chmod(path, Convert.ToInt32("600", 8));
            }
            catch (Exception e)
            {
                Debug.LogWarning($"Prefabric: cannot restrict {path}: {e.Message}");
            }
        }

        static Dictionary<string, LinkServer.Handler> Handlers() => new Dictionary<string, LinkServer.Handler>
        {
            ["editor.state"] = _ => State(),
            // Play mode changes take effect on a later frame; `editor/stateChanged` follows.
            ["editor.play"] = _ =>
            {
                EditorApplication.isPlaying = true;
                return State();
            },
            ["editor.stop"] = _ =>
            {
                EditorApplication.isPlaying = false;
                return State();
            },
            ["editor.pause"] = p =>
            {
                EditorApplication.isPaused = p.TryGetValue("paused", out var v) && v is bool paused ? paused : !EditorApplication.isPaused;
                return State();
            },
            ["editor.step"] = _ =>
            {
                EditorApplication.Step();
                return State();
            },
            ["assets.refresh"] = _ =>
            {
                AssetDatabase.Refresh();
                return null;
            },
            ["asset.ping"] = p => Show(AssetAt(p)),
            ["object.select"] = p => SelectObject(p.TryGetValue("path", out var path) ? path as string : null,
                p.TryGetValue("fileId", out var id) ? id as string : null),
        };

        /// <summary>The asset named by <c>path</c> or, failing that, <c>guid</c>.</summary>
        static UnityEngine.Object AssetAt(Dictionary<string, object> p)
        {
            var path = p.TryGetValue("path", out var v) ? v as string : null;
            if (string.IsNullOrEmpty(path) && p.TryGetValue("guid", out var g) && g is string guid)
                path = AssetDatabase.GUIDToAssetPath(guid);
            return string.IsNullOrEmpty(path) ? null : AssetDatabase.LoadMainAssetAtPath(path);
        }

        static bool Show(UnityEngine.Object target)
        {
            if (target == null)
                return false;
            Selection.activeObject = target;
            EditorGUIUtility.PingObject(target);
            return true;
        }

        /// <summary>A GameObject or component of a scene or prefab by its file ID. A scene object is
        /// found only while its scene is open; otherwise the scene asset is pinged.</summary>
        internal static bool SelectObject(string path, string fileId)
        {
            if (string.IsNullOrEmpty(path) || string.IsNullOrEmpty(fileId))
                return false;
            var guid = AssetDatabase.AssetPathToGUID(path);
            if (string.IsNullOrEmpty(guid))
                return false;
            var isScene = path.EndsWith(".unity", StringComparison.OrdinalIgnoreCase);
            // GlobalObjectId: 1 = an object of an asset (prefab, ScriptableObject), 2 = a scene object.
            var kind = isScene ? 2 : 1;
            UnityEngine.Object target = null;
            if (GlobalObjectId.TryParse($"GlobalObjectId_V1-{kind}-{guid}-{UnsignedFileId(fileId)}-0", out var id))
                target = GlobalObjectId.GlobalObjectIdentifierToObjectSlow(id);
            if (target is Component component)
                target = component.gameObject;
            if (target != null)
                return Show(target);
            Show(AssetDatabase.LoadMainAssetAtPath(path));
            return false;
        }

        /// <summary>YAML writes file IDs signed; <c>GlobalObjectId</c> parses them unsigned.</summary>
        static string UnsignedFileId(string fileId) =>
            long.TryParse(fileId, out var signed) ? unchecked((ulong)signed).ToString() : fileId;

        /// <summary>The state the IDE shows: play mode, compiling, importing.</summary>
        public static Dictionary<string, object> State() => new Dictionary<string, object>
        {
            ["playMode"] = EditorApplication.isPlaying ? (EditorApplication.isPaused ? "paused" : "playing") : "edit",
            ["compiling"] = EditorApplication.isCompiling,
            ["updating"] = EditorApplication.isUpdating,
        };

        static Dictionary<string, object> cachedHello;
        static readonly List<object> compiling = new List<object>();

        static void Collect(CompilerMessage[] messages)
        {
            foreach (var m in messages)
            {
                if (m.type != CompilerMessageType.Error && m.type != CompilerMessageType.Warning)
                    continue;
                compiling.Add(new Dictionary<string, object>
                {
                    ["file"] = m.file?.Replace('\\', '/') ?? "",
                    ["line"] = m.line,
                    ["column"] = m.column,
                    ["severity"] = m.type == CompilerMessageType.Error ? "error" : "warning",
                    ["message"] = m.message,
                });
            }
        }

        /// <summary>Sends the compile's messages; kept in <c>SessionState</c> for clients that connect later.</summary>
        static void CompileFinished()
        {
            var result = new Dictionary<string, object> { ["messages"] = new List<object>(compiling) };
            compiling.Clear();
            SessionState.SetString(CompileKey, Json.Write(result));
            cachedHello = null;
            PublishState();
            server.Notify("compile/finished", result);
        }

        static Dictionary<string, object> LastCompile() =>
            Json.Parse(SessionState.GetString(CompileKey, "{\"messages\":[]}")) as Dictionary<string, object>;

        /// <summary>Called on a socket thread: answers from what the main thread last saw.</summary>
        static Dictionary<string, object> Hello() => new Dictionary<string, object>(cachedHello ?? new Dictionary<string, object>());

        const int MaxLogBatch = 500;
        const double LogInterval = 0.1;
        /// <summary>Kept while no IDE is connected, so the IDE sees what led up to connecting.</summary>
        const int MaxQueuedLogs = 1000;
        /// <summary>Beyond this the oldest entries are dropped even while connected: a game that logs
        /// faster than the link sends must not grow the Editor's memory.</summary>
        const int MaxBacklog = 20000;
        static readonly ConcurrentQueue<Dictionary<string, object>> logs = new ConcurrentQueue<Dictionary<string, object>>();
        /// <summary>Entries in <see cref="logs"/>. Mono's <c>ConcurrentQueue.Count</c> walks the queue,
        /// too slow for a callback that runs on every log.</summary>
        static int queued;
        static double lastLogFlush;

        /// <summary>Any thread, on every log: keep it cheap. Batched to the IDE from <see cref="Update"/>.</summary>
        static void OnLog(string message, string stackTrace, LogType type)
        {
            logs.Enqueue(new Dictionary<string, object>
            {
                ["kind"] = type switch
                {
                    LogType.Warning => "warning",
                    LogType.Error => "error",
                    LogType.Exception => "exception",
                    LogType.Assert => "assert",
                    _ => "log",
                },
                ["message"] = message,
                ["stackTrace"] = stackTrace ?? "",
                ["time"] = DateTime.Now.ToString("HH:mm:ss.fff"),
            });
            var count = Interlocked.Increment(ref queued);
            var limit = server.ClientCount == 0 ? MaxQueuedLogs : MaxBacklog;
            if (count > limit && logs.TryDequeue(out _))
                Interlocked.Decrement(ref queued);
        }

        static void FlushLogs(int max)
        {
            // Only to IDEs past the handshake: Notify skips the others, and the entries would be lost.
            if (Volatile.Read(ref queued) == 0 || !server.HasReadyClient)
                return;
            while (max > 0)
            {
                var batch = new List<object>();
                while (batch.Count < MaxLogBatch && batch.Count < max && logs.TryDequeue(out var entry))
                {
                    Interlocked.Decrement(ref queued);
                    batch.Add(entry);
                }
                if (batch.Count == 0)
                    return;
                max -= batch.Count;
                server.Notify("console/logs", new Dictionary<string, object> { ["entries"] = batch });
            }
        }

        static void Update()
        {
            server.Pump();
            var now = EditorApplication.timeSinceStartup;
            if (now - lastLogFlush >= LogInterval)
            {
                lastLogFlush = now;
                FlushLogs(MaxLogBatch);
            }
            // Polled as well: `isCompiling`/`isUpdating` change without an event.
            var state = State();
            if (lastState == null || !SameState(state, lastState))
                PublishState(state);
        }

        static void PublishState() => PublishState(State());

        static void PublishState(Dictionary<string, object> state)
        {
            if (lastState != null && cachedHello != null && SameState(state, lastState))
                return;
            lastState = state;
            cachedHello = new Dictionary<string, object>
            {
                ["unityVersion"] = Application.unityVersion,
                ["projectPath"] = Directory.GetParent(Application.dataPath)!.FullName,
                ["state"] = state,
                ["compile"] = LastCompile(),
            };
            server.Notify("editor/stateChanged", state);
        }

        static bool SameState(Dictionary<string, object> a, Dictionary<string, object> b)
        {
            foreach (var pair in a)
                if (!b.TryGetValue(pair.Key, out var v) || !Equals(v, pair.Value))
                    return false;
            return true;
        }
    }
}
