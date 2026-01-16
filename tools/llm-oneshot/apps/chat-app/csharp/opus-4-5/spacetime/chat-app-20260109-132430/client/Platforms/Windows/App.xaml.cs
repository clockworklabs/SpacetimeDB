using Microsoft.UI.Xaml;

namespace ChatClient.WinUI;

public partial class App : MauiWinUIApplication
{
    public App()
    {
        this.InitializeComponent();
    }

    protected override MauiApp CreateMauiApp() => ChatClient.MauiProgram.CreateMauiApp();
}
