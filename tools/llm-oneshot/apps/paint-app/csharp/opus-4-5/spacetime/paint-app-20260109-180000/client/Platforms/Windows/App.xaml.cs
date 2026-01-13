namespace PaintApp.Client.WinUI;

public partial class App : MauiWinUIApplication
{
    public App()
    {
        this.InitializeComponent();
    }

    protected override MauiApp CreateMauiApp() => PaintApp.Client.MauiProgram.CreateMauiApp();
}
