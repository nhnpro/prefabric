using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Net;
using System.Net.NetworkInformation;
using System.Net.Sockets;
using System.Text;
using NUnit.Framework;
using Prefabric.Unity.Editor.Link;

namespace Prefabric.Unity.Editor.Tests
{
	/// <summary>The link's security rules (plan task E9) and handshake.</summary>
	public class LinkServerTests
	{
		const string Token = "0123456789abcdef0123456789abcdef";
		LinkServer server;

		[SetUp]
		public void Start()
		{
			var handlers = new Dictionary<string, LinkServer.Handler> { ["echo"] = p => p.TryGetValue("value", out var v) ? v : null };
			server = new LinkServer(0, Token, handlers, () => new Dictionary<string, object> { ["unityVersion"] = "test" });
		}

		[TearDown]
		public void Stop() => server.Dispose();

		sealed class Connection : IDisposable
		{
			readonly TcpClient tcp;
			readonly StreamReader reader;
			readonly Stream stream;

			public Connection(int port)
			{
				tcp = new TcpClient();
				tcp.Connect(IPAddress.Loopback, port);
				tcp.ReceiveTimeout = 5000;
				stream = tcp.GetStream();
				reader = new StreamReader(stream, new UTF8Encoding(false));
			}

			public void Send(string line)
			{
				var bytes = Encoding.UTF8.GetBytes(line + "\n");
				stream.Write(bytes, 0, bytes.Length);
			}

			public Dictionary<string, object> Receive()
			{
				var line = reader.ReadLine();
				return line == null ? null : (Dictionary<string, object>)Json.Parse(line);
			}

			public bool Closed
			{
				get
				{
					try
					{
						return reader.ReadLine() == null;
					}
					catch (IOException)
					{
						return true;
					}
				}
			}

			public void Dispose() => tcp.Close();
		}

		static int ErrorCode(Dictionary<string, object> reply) =>
			(int)(double)((Dictionary<string, object>)reply["error"])["code"];

		string Hello(string token) => Json.Write(new Dictionary<string, object>
		{
			["jsonrpc"] = "2.0",
			["id"] = 1,
			["method"] = "hello",
			["params"] = new Dictionary<string, object> { ["token"] = token, ["protocol"] = 1 },
		});

		[Test]
		public void RefusesAConnectionWithoutTheToken()
		{
			using var c = new Connection(server.Port);
			c.Send(Hello("ffffffffffffffffffffffffffffffff"));
			Assert.AreEqual(LinkServer.BadToken, ErrorCode(c.Receive()));
			Assert.IsTrue(c.Closed);
		}

		[Test]
		public void RefusesRequestsBeforeTheHandshake()
		{
			using var c = new Connection(server.Port);
			c.Send("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"echo\",\"params\":{\"value\":1}}");
			Assert.AreEqual(LinkServer.HandshakeRequired, ErrorCode(c.Receive()));
			Assert.IsTrue(c.Closed);
		}

		[Test]
		public void AnswersAfterTheHandshake()
		{
			using var c = new Connection(server.Port);
			c.Send(Hello(Token));
			var hello = (Dictionary<string, object>)c.Receive()["result"];
			Assert.AreEqual(1.0, hello["protocol"]);
			Assert.AreEqual("test", hello["unityVersion"]);

			c.Send("{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"echo\",\"params\":{\"value\":\"hi\"}}");
			// Requests run on the main thread: pump until the reply is queued.
			for (var i = 0; i < 50; i++)
			{
				server.Pump();
				System.Threading.Thread.Sleep(10);
			}
			Assert.AreEqual("hi", c.Receive()["result"]);

			server.Notify("editor/stateChanged", new Dictionary<string, object> { ["playMode"] = "edit" });
			Assert.AreEqual("editor/stateChanged", c.Receive()["method"]);
		}

		[Test]
		public void AClientThatStopsReadingNeverBlocksTheEditor()
		{
			using var c = new Connection(server.Port);
			c.Send(Hello(Token));
			c.Receive();
			// Far more than the socket buffers hold: Notify runs on the Editor's main thread.
			var entries = new List<object> { new string('x', 4000) };
			var watch = System.Diagnostics.Stopwatch.StartNew();
			for (var i = 0; i < 5000; i++)
				server.Notify("console/logs", new Dictionary<string, object> { ["entries"] = entries });
			Assert.Less(watch.ElapsedMilliseconds, 2000);
			// Too far behind: disconnected (it reconnects).
			for (var i = 0; i < 100 && server.ClientCount > 0; i++)
				System.Threading.Thread.Sleep(20);
			Assert.AreEqual(0, server.ClientCount);
		}

		[Test]
		public void DropsAPeerThatNeverSaysHello()
		{
			using var c = new Connection(server.Port);
			// The handshake timeout is 5 s; the connection's read times out first otherwise.
			System.Threading.Thread.Sleep(5500);
			Assert.IsTrue(c.Closed);
			Assert.AreEqual(0, server.ClientCount);
		}

		[Test]
		public void DropsALongLineBeforeHello()
		{
			using var c = new Connection(server.Port);
			c.Send(new string('x', 100 * 1024));
			Assert.IsTrue(c.Closed);
		}

		[Test]
		public void RefusesDeeplyNestedJson()
		{
			Assert.Throws<FormatException>(() => Json.Parse(new string('[', 10000) + new string(']', 10000)));
			Assert.IsNotNull(Json.Parse(new string('[', 32) + new string(']', 32)));
		}

		[Test]
		public void KeepsAcceptingAfterAPeerResets()
		{
			var reset = new TcpClient { LingerState = new LingerOption(true, 0) };
			reset.Connect(IPAddress.Loopback, server.Port);
			reset.Close();
			using var c = new Connection(server.Port);
			c.Send(Hello(Token));
			Assert.IsTrue(c.Receive().ContainsKey("result"));
		}

		[Test]
		public void ListensOnLoopbackOnly()
		{
			var address = NetworkInterface.GetAllNetworkInterfaces()
				.Where(n => n.OperationalStatus == OperationalStatus.Up)
				.SelectMany(n => n.GetIPProperties().UnicastAddresses)
				.Select(a => a.Address)
				.FirstOrDefault(a => a.AddressFamily == AddressFamily.InterNetwork && !IPAddress.IsLoopback(a));
			if (address == null)
				Assert.Ignore("no non-loopback IPv4 address on this machine");
			using var tcp = new TcpClient();
			Assert.Throws<SocketException>(() => tcp.Connect(address, server.Port));
		}

		[Test]
		public void TokensAreRandom128BitHex()
		{
			var a = LinkServer.NewToken();
			Assert.AreEqual(32, a.Length);
			Assert.IsTrue(a.All(Uri.IsHexDigit));
			Assert.AreNotEqual(a, LinkServer.NewToken());
		}

		[Test]
		public void JsonRoundTrips()
		{
			var text = "{\"a\":[1,2.5,\"x\\n\\\"y\\\"\"],\"b\":{\"c\":true,\"d\":null}}";
			Assert.AreEqual(text, Json.Write(Json.Parse(text)));
		}
	}
}
