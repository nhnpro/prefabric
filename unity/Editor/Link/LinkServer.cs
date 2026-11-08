using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.IO;
using System.Net;
using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text;
using System.Threading;

namespace Prefabric.Unity.Editor.Link
{
    /// <summary>
    /// The Editor side of the link (<c>docs/unity-link.md</c>): newline-delimited JSON-RPC 2.0 on
    /// <c>127.0.0.1</c>, a token handshake, up to <see cref="MaxClients"/> clients. Socket I/O runs
    /// on background threads; requests run on the main thread through <see cref="Pump"/>.
    /// </summary>
    internal sealed class LinkServer : IDisposable
    {
        public const int Protocol = 1;
        public const int MaxClients = 4;

        public const int BadToken = -32001;
        public const int HandshakeRequired = -32002;
        public const int TooManyClients = -32003;
        const int MethodNotFound = -32601;
        const int InternalError = -32603;

        /// <summary>Handles a request on the main thread; returns its result or throws.</summary>
        public delegate object Handler(Dictionary<string, object> parameters);

        readonly string token;
        readonly Dictionary<string, Handler> handlers;
        readonly Func<Dictionary<string, object>> helloResult;
        readonly ConcurrentQueue<Action> mainThread = new ConcurrentQueue<Action>();
        readonly List<Client> clients = new List<Client>();
        readonly TcpListener listener;
        readonly Thread acceptThread;
        volatile bool stopped;

        public int Port { get; }

        public LinkServer(int port, string token, Dictionary<string, Handler> handlers, Func<Dictionary<string, object>> helloResult)
        {
            this.token = token;
            this.handlers = handlers;
            this.helloResult = helloResult;
            listener = Listen(port);
            Port = ((IPEndPoint)listener.LocalEndpoint).Port;
            acceptThread = new Thread(AcceptLoop) { IsBackground = true, Name = "Prefabric link" };
            acceptThread.Start();
        }

        /// <summary>Loopback only. The previous port is reused after a domain reload when it's free.</summary>
        static TcpListener Listen(int port)
        {
            try
            {
                var l = new TcpListener(IPAddress.Loopback, port);
                l.Server.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);
                l.Start();
                return l;
            }
            catch (SocketException) when (port != 0)
            {
                return Listen(0);
            }
        }

        public static string NewToken()
        {
            var bytes = new byte[16];
            using (var rng = RandomNumberGenerator.Create())
                rng.GetBytes(bytes);
            var sb = new StringBuilder(32);
            foreach (var b in bytes)
                sb.Append(b.ToString("x2"));
            return sb.ToString();
        }

        static bool SameToken(string a, string b)
        {
            if (a == null || b == null || a.Length != b.Length)
                return false;
            var diff = 0;
            for (var i = 0; i < a.Length; i++)
                diff |= a[i] ^ b[i];
            return diff == 0;
        }

        /// <summary>Runs queued requests. Call from <c>EditorApplication.update</c>.</summary>
        public void Pump()
        {
            while (mainThread.TryDequeue(out var action))
                action();
        }

        public int ClientCount
        {
            get { lock (clients) return clients.Count; }
        }

        public bool HasReadyClient
        {
            get
            {
                lock (clients)
                    foreach (var c in clients)
                        if (c.Ready)
                            return true;
                return false;
            }
        }

        /// <summary>Sends a notification to every client that completed the handshake.</summary>
        public void Notify(string method, object parameters)
        {
            var line = Json.Write(new Dictionary<string, object> { ["jsonrpc"] = "2.0", ["method"] = method, ["params"] = parameters });
            Client[] targets;
            lock (clients)
                targets = clients.ToArray();
            foreach (var c in targets)
                if (c.Ready)
                    c.Send(line);
        }

        void AcceptLoop()
        {
            while (!stopped)
            {
                Client client;
                try
                {
                    var tcp = listener.AcceptTcpClient();
                    try
                    {
                        client = new Client(this, tcp);
                    }
                    catch (Exception)
                    {
                        // The peer reset before we could set it up.
                        tcp.Close();
                        continue;
                    }
                }
                catch (ObjectDisposedException)
                {
                    return;
                }
                catch (Exception)
                {
                    // A transient accept error (a connection aborted while queued): keep listening.
                    if (stopped)
                        return;
                    Thread.Sleep(50);
                    continue;
                }
                lock (clients)
                {
                    if (clients.Count >= MaxClients)
                    {
                        client.Send(Error(null, TooManyClients, "too many clients"));
                        client.Close();
                        continue;
                    }
                    clients.Add(client);
                }
                client.Start();
            }
        }

        void Remove(Client client)
        {
            lock (clients)
                clients.Remove(client);
        }

        static string Response(object id, object result) =>
            Json.Write(new Dictionary<string, object> { ["jsonrpc"] = "2.0", ["id"] = id, ["result"] = result });

        static string Error(object id, int code, string message) =>
            Json.Write(new Dictionary<string, object>
            {
                ["jsonrpc"] = "2.0",
                ["id"] = id,
                ["error"] = new Dictionary<string, object> { ["code"] = code, ["message"] = message },
            });

        /// <summary>Handles one line; returns false to close the connection.</summary>
        bool Handle(Client client, string line)
        {
            Dictionary<string, object> message;
            try
            {
                message = Json.Parse(line) as Dictionary<string, object>;
            }
            catch (FormatException e)
            {
                client.Send(Error(null, -32700, e.Message));
                return client.Ready;
            }
            if (message == null)
                return client.Ready;
            message.TryGetValue("id", out var id);
            message.TryGetValue("method", out var methodValue);
            var method = methodValue as string;
            var parameters = message.TryGetValue("params", out var p) ? p as Dictionary<string, object> : null;
            parameters ??= new Dictionary<string, object>();

            if (!client.Ready)
            {
                if (method != "hello")
                {
                    client.Send(Error(id, HandshakeRequired, "send hello first"));
                    return false;
                }
                parameters.TryGetValue("token", out var given);
                if (!SameToken(given as string, token))
                {
                    client.Send(Error(id, BadToken, "bad token"));
                    return false;
                }
                var wanted = parameters.TryGetValue("protocol", out var v) && v is double d ? (int)d : Protocol;
                var result = helloResult();
                result["protocol"] = Math.Min(wanted, Protocol);
                client.Ready = true;
                client.Send(Response(id, result));
                return true;
            }

            Handler handler = null;
            bool known;
            // Other assemblies register handlers on the main thread (`Link.Register`).
            lock (handlers)
                known = method != null && handlers.TryGetValue(method, out handler);
            if (!known)
            {
                if (id != null)
                    client.Send(Error(id, MethodNotFound, $"unknown method {method}"));
                return true;
            }
            mainThread.Enqueue(() =>
            {
                string reply;
                try
                {
                    reply = Response(id, handler(parameters));
                }
                catch (Exception e)
                {
                    reply = Error(id, InternalError, e.Message);
                }
                if (id != null)
                    client.Send(reply);
            });
            return true;
        }

        public void Dispose()
        {
            stopped = true;
            try
            {
                listener.Stop();
            }
            catch (SocketException)
            {
            }
            Client[] all;
            lock (clients)
            {
                all = clients.ToArray();
                clients.Clear();
            }
            foreach (var c in all)
                c.Close();
        }

        sealed class Client
        {
            /// <summary>Lines waiting to be written; a client this far behind is disconnected rather
            /// than left to grow the Editor's memory (it reconnects).</summary>
            const int MaxPending = 2000;
            /// <summary>Before the handshake a peer is unknown: it gets little time and little room.</summary>
            const int HandshakeTimeoutMs = 5000;
            const int MaxLineBeforeHello = 64 * 1024;
            const int MaxLine = 16 * 1024 * 1024;

            readonly LinkServer server;
            readonly TcpClient tcp;
            readonly NetworkStream stream;
            readonly ConcurrentQueue<byte[]> outgoing = new ConcurrentQueue<byte[]>();
            readonly SemaphoreSlim pending = new SemaphoreSlim(0);
            volatile bool closing;
            public volatile bool Ready;
            /// <summary>Kept referenced so it isn't collected before it fires.</summary>
            Timer handshake;

            public Client(LinkServer server, TcpClient tcp)
            {
                this.server = server;
                this.tcp = tcp;
                tcp.NoDelay = true;
                stream = tcp.GetStream();
                new Thread(WriteLoop) { IsBackground = true, Name = "Prefabric link writer" }.Start();
            }

            public void Start()
            {
                new Thread(ReadLoop) { IsBackground = true, Name = "Prefabric link client" }.Start();
                // A peer that never completes the handshake doesn't keep a client slot.
                handshake = new Timer(_ =>
                {
                    if (!Ready)
                        Abort();
                    handshake?.Dispose();
                }, null, HandshakeTimeoutMs, Timeout.Infinite);
            }

            void ReadLoop()
            {
                try
                {
                    var buffer = new byte[4096];
                    var line = new MemoryStream();
                    int read;
                    while ((read = stream.Read(buffer, 0, buffer.Length)) > 0)
                    {
                        var start = 0;
                        for (var i = 0; i < read; i++)
                        {
                            if (buffer[i] != (byte)'\n')
                                continue;
                            line.Write(buffer, start, i - start);
                            start = i + 1;
                            var text = Encoding.UTF8.GetString(line.GetBuffer(), 0, (int)line.Length).TrimEnd('\r');
                            line.SetLength(0);
                            if (text.Length > 0 && !server.Handle(this, text))
                                return;
                        }
                        line.Write(buffer, start, read - start);
                        if (line.Length > (Ready ? MaxLine : MaxLineBeforeHello))
                            return;
                    }
                }
                catch (Exception)
                {
                    // The IDE went away or the server stopped.
                }
                finally
                {
                    server.Remove(this);
                    Close();
                }
            }

            /// <summary>Writes on its own thread, so a slow IDE never blocks the Editor's main thread.
            /// After <see cref="Close"/> it writes what is queued (an error reply), then closes.</summary>
            void WriteLoop()
            {
                try
                {
                    while (true)
                    {
                        pending.Wait();
                        if (outgoing.TryDequeue(out var bytes))
                            stream.Write(bytes, 0, bytes.Length);
                        else if (closing)
                            break;
                    }
                }
                catch (Exception)
                {
                    // The IDE went away.
                }
                finally
                {
                    closing = true;
                    try
                    {
                        tcp.Close();
                    }
                    catch (Exception)
                    {
                    }
                }
            }

            /// <summary>Queues a line; never blocks.</summary>
            public void Send(string line)
            {
                if (closing)
                    return;
                if (pending.CurrentCount >= MaxPending)
                {
                    Abort();
                    return;
                }
                outgoing.Enqueue(Encoding.UTF8.GetBytes(line + "\n"));
                pending.Release();
            }

            /// <summary>Closes once the queued lines are written.</summary>
            public void Close()
            {
                if (closing)
                    return;
                closing = true;
                pending.Release();
            }

            /// <summary>Closes now, dropping what is queued.</summary>
            public void Abort()
            {
                closing = true;
                while (outgoing.TryDequeue(out _))
                {
                }
                pending.Release();
                try
                {
                    tcp.Close();
                }
                catch (Exception)
                {
                }
            }
        }
    }
}
