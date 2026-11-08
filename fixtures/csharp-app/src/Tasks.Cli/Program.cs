using Tasks.Core;

var store = new TaskStore();
store.Add("Buy milk", Priority.High);
store.Add("Write the report");
store.Add("Call the plumber", Priority.Low);
store.Complete(2);

var command = args.Length > 0 ? args[0] : "list";
var items = command switch
{
    "search" when args.Length > 1 => store.Search(args[1]),
    "done" => store.Items.Where(t => t.Done),
    _ => store.Pending(),
};

foreach (var item in items)
{
    // Set a breakpoint here (QA: debugger) and inspect `item`
    Console.WriteLine(item);
}
Console.WriteLine($"{store.CountDone()} of {store.Items.Count} done");

// `wait`: keep running until Enter, so a debugger can attach (QA: Attach to Process).
if (command == "wait")
{
    Console.WriteLine($"pid {Environment.ProcessId}: press Enter to exit");
    Console.ReadLine();
}
