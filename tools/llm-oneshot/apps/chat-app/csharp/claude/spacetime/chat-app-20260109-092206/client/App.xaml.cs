namespace ChatApp;

public partial class App : Application
{
    public App()
    {
        // Add global exception handlers
        AppDomain.CurrentDomain.UnhandledException += (s, e) =>
        {
            Console.WriteLine($"Unhandled exception: {e.ExceptionObject}");
            System.Diagnostics.Debug.WriteLine($"Unhandled exception: {e.ExceptionObject}");
        };
        
        TaskScheduler.UnobservedTaskException += (s, e) =>
        {
            Console.WriteLine($"Unobserved task exception: {e.Exception}");
            System.Diagnostics.Debug.WriteLine($"Unobserved task exception: {e.Exception}");
        };

        try
        {
            InitializeComponent();
        }
        catch (Exception ex)
        {
            Console.WriteLine($"InitializeComponent failed: {ex}");
            System.Diagnostics.Debug.WriteLine($"InitializeComponent failed: {ex}");
        }
    }

    protected override Window CreateWindow(IActivationState? activationState)
    {
        try
        {
            var page = new MainPage();
            return new Window(page)
            {
                Title = "SpacetimeDB Chat",
                Width = 1100,
                Height = 700
            };
        }
        catch (Exception ex)
        {
            Console.WriteLine($"CreateWindow failed: {ex}");
            System.Diagnostics.Debug.WriteLine($"CreateWindow failed: {ex}");
            throw;
        }
    }
}
