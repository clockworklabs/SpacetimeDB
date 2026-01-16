using Microsoft.UI.Xaml;

namespace ChatApp.WinUI;

public partial class App : MauiWinUIApplication
{
    public App()
    {
        this.InitializeComponent();
    }

    protected override MauiApp CreateMauiApp() => ChatApp.MauiProgram.CreateMauiApp();
}
