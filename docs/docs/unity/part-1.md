# Unity Tutorial - Basic Multiplayer - Part 1 - Setup

![UnityTutorial-HeroImage](/images/unity-tutorial/UnityTutorial-HeroImage.JPG)

Need help with the tutorial? [Join our Discord server](https://discord.gg/spacetimedb)!

## Prepare Project Structure

This project is separated into two sub-projects;

1. Server (module) code
2. Client code

First, we'll create a project root directory (you can choose the name):

```bash
mkdir SpacetimeDBUnityTutorial
cd SpacetimeDBUnityTutorial
```

We'll start by populating the client directory.

## Setting up the Tutorial Unity Project

In this section, we will guide you through the process of setting up a Unity Project that will serve as the starting point for our tutorial. By the end of this section, you will have a basic Unity project and be ready to implement the server functionality.

### Step 1: Create a Blank Unity Project

Open Unity and create a new project by selecting "New" from the Unity Hub or going to **File -> New Project**.

![UnityHub-NewProject](/images/unity-tutorial/UnityHub-NewProject.JPG)

**⚠️ Important: Ensure `3D (URP)` is selected** to properly render the materials in the scene!

For Project Name use `client`. For Project Location make sure that you use your `SpacetimeDBUnityTutorial` directory. This is the directory that we created in a previous step.

![UnityHub-3DURP](/images/unity-tutorial/UnityHub-3DURP.JPG)

Click "Create" to generate the blank project.

### Step 2: Adding Required Packages

To work with SpacetimeDB and ensure compatibility, we need to add some essential packages to our Unity project. Follow these steps:

1. Open the Unity Package Manager by going to **Window -> Package Manager**.
2. In the Package Manager window, select the "Unity Registry" tab to view unity packages.
3. Search for and install the following package:
   - **Input System**: Enables the use of Unity's new Input system used by this project.

![PackageManager-InputSystem](/images/unity-tutorial/PackageManager-InputSystem.JPG)

4. You may need to restart the Unity Editor to switch to the new Input system.

![PackageManager-Restart](/images/unity-tutorial/PackageManager-Restart.JPG)

### Step 3: Importing the Tutorial Package

In this step, we will import the provided Unity tutorial package that contains the basic single-player game setup. Follow these instructions:

1. Download the tutorial package from the releases page on GitHub: [https://github.com/clockworklabs/SpacetimeDBUnityTutorial/releases/latest](https://github.com/clockworklabs/SpacetimeDBUnityTutorial/releases/latest)
2. In Unity, go to **Assets -> Import Package -> Custom Package**.

![Unity-ImportCustomPackageB](/images/unity-tutorial/Unity-ImportCustomPackageB.JPG)

3. Browse and select the downloaded tutorial package file.
4. Unity will prompt you with an import settings dialog. Ensure that all the files are selected and click "Import" to import the package into your project.
5. At this point in the project, you shouldn't have any errors.

![Unity-ImportCustomPackage2](/images/unity-tutorial/Unity-ImportCustomPackage2.JPG)

### Step 4: Running the Project

Now that we have everything set up, let's run the project and see it in action:

1. Open the scene named "Main" in the Scenes folder provided in the project hierarchy by double-clicking it.

![Unity-OpenSceneMain](/images/unity-tutorial/Unity-OpenSceneMain.JPG)

**NOTE:** When you open the scene you may get a message saying you need to import TMP Essentials. When it appears, click the "Import TMP Essentials" button.

🧹 Clear any false-positive TMPro errors that may show.

![Unity Import TMP Essentials](/images/unity-tutorial/Unity-ImportTMPEssentials.JPG)

2. Press the **Play** button located at the top of the Unity Editor.

![Unity-Play](/images/unity-tutorial/Unity-Play.JPG)

3. Enter any name and click "Continue"

4. You should see a character loaded in the scene, and you can use the keyboard or mouse controls to move the character around.

Congratulations! You have successfully set up the basic single-player game project. In the next section, we will start integrating SpacetimeDB functionality to enable multiplayer features.

## Writing our SpacetimeDB Server Module

At this point you should have the single player game working. In your CLI, your current working directory should be within your `SpacetimeDBUnityTutorial` directory that we created in a previous step.

### Create the Module

1. It is important that you already have the SpacetimeDB CLI tool [installed](/install).

2. Run SpacetimeDB locally using the installed CLI. In a **new** terminal or command window, run the following command:

```bash
spacetime start
```

💡 Standalone mode will run in the foreground.
💡 Below examples Rust language, [but you may also use C#](../modules/c-sharp).

### The Entity Component Systems (ECS)

Before we continue to creating the server module, it's important to understand the basics of the ECS. This is a game development architecture that separates game objects into components for better flexibility and performance. You can read more about the ECS design pattern [here](https://en.wikipedia.org/wiki/Entity_component_system).

We chose ECS for this example project because it promotes scalability, modularity, and efficient data management, making it ideal for building multiplayer games with SpacetimeDB.

### Create the Server Module

From here, the tutorial continues with your favorite server module language of choice:

- [Rust](part-2a-rust)
- [C#](part-2b-c-sharp)
