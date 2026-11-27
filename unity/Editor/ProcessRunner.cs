/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

using System;
using System.Diagnostics;
using System.Text;
using System.Threading;
using System.Threading.Tasks;

namespace Prefabric.Unity.Editor
{
	internal class ProcessRunnerResult
	{
		public bool Success { get; set; }
		public string Output { get; set; }
		public string Error { get; set; }
	}

	internal static class ProcessRunner
	{
		public const int DefaultTimeoutInMilliseconds = 300000;

		public static ProcessStartInfo ProcessStartInfoFor(string filename, string arguments, bool redirect = true, bool shell = false)
		{
			return new ProcessStartInfo
			{
				UseShellExecute = shell,
				CreateNoWindow = true, 
				RedirectStandardOutput = redirect,
				RedirectStandardError = redirect,
				FileName = filename,
				Arguments = arguments
			};
		}

		public static bool Start(string filename, string arguments)
		{
			return Start(ProcessStartInfoFor(filename, arguments, false));
		}

		public static bool Start(ProcessStartInfo processStartInfo)
		{
			var process = new Process { StartInfo = processStartInfo };

			using (process)
			{
				return process.Start();
			}
		}

		public static ProcessRunnerResult StartAndWaitForExit(string filename, string arguments, int timeoutms = DefaultTimeoutInMilliseconds, Action<string> onOutputReceived = null)
		{
			return StartAndWaitForExit(ProcessStartInfoFor(filename, arguments), timeoutms, onOutputReceived);
		}

		public static ProcessRunnerResult StartAndWaitForExit(ProcessStartInfo processStartInfo, int timeoutms = DefaultTimeoutInMilliseconds, Action<string> onOutputReceived = null)
		{
			var process = new Process { StartInfo = processStartInfo };

			using (process)
			{
				var outputLock = new object();
				var errorLock = new object();
				var sbOutput = new StringBuilder();
				var sbError = new StringBuilder();

				var outputSource = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
				var errorSource = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);

				process.OutputDataReceived += (_, e) =>
				{
					Append(sbOutput, outputLock, e.Data, outputSource);
					if (onOutputReceived != null && e.Data != null)
						onOutputReceived(e.Data);
				};
				process.ErrorDataReceived += (_, e) => Append(sbError, errorLock, e.Data, errorSource);

				if (!process.Start())
					return new ProcessRunnerResult { Success = false, Error = string.Empty, Output = string.Empty };
				process.BeginOutputReadLine();
				process.BeginErrorReadLine();

				var exited = process.WaitForExit(timeoutms);
				if (exited)
				{
					using (var cts = new CancellationTokenSource())
					{
						var drained = Task.WhenAll(outputSource.Task, errorSource.Task);
						Task.WhenAny(drained, Task.Delay(5000, cts.Token)).Wait();
						cts.Cancel();
					}
					string output, error;
					lock (outputLock) output = sbOutput.ToString();
					lock (errorLock) error = sbError.ToString();
					return new ProcessRunnerResult { Success = process.ExitCode == 0, Error = error, Output = output };
				}

				try
				{
					process.Kill();
					process.WaitForExit(5000);
				}
				catch
				{
					/* ignore */
				}
				using (var cts = new CancellationTokenSource())
				{
					var drained = Task.WhenAll(outputSource.Task, errorSource.Task);
					Task.WhenAny(drained, Task.Delay(1000, cts.Token)).Wait();
					cts.Cancel();
				}

				string timedOutOutput, timedOutError;
				lock (outputLock) timedOutOutput = sbOutput.ToString();
				lock (errorLock) timedOutError = sbError.ToString();
				return new ProcessRunnerResult { Success = false, Error = timedOutError, Output = timedOutOutput };
			}
		}

		private static void Append(StringBuilder sb, object sync, string data, TaskCompletionSource<bool> taskSource)
		{
			if (data == null)
			{
				taskSource.TrySetResult(true);
				return;
			}

			lock (sync)
				sb?.AppendLine(data);
		}
	}
}
