## Unity Testsuite

This testsuite is designed to test the SDK against a real Unity project. This project was initially branched off of the [SpacetimeDBCircleGame](https://github.com/clockworklabs/SpacetimeDBCircleGame)

### Running Locally

1. clone this repository
2. Open UnityHub
3. Make sure you have a valid Unity license
4. Add -> `com.clockworklabs.spacetimedbsdk/unity-tests/client`
5. Open the project called `client` at the top of your project list

Now, while that's loading you can get SpacetimeDB setup. The easiest thing to do will be this:
```
# cd into your unity-tests directory
cd com.clockworklabs.spacetimedbsdk/unity-tests
# clone a new instance of SpacetimeDB within this directory
git clone ssh://git@github.com/clockworklabs/SpacetimeDB
cd SpacetimeDB
git checkout a-branch-I-want-to-test-against
# You probably want to install this version of the CLI for generation + publishing
cargo install --path ./crates/cli
# start spacetimedb, Boppy recommends starting this in a separate terminal window
spacetime start &

# generate bindings
bash server/generate.sh

# publish module
bash server/publish.sh
```

After you've done this you can open `Window -> General -> Test Runner` which will open the test runner interface. If you look in `Play Mode Tests` you should see some tests you can run. You can run tests individually, run selected tests or run all tests.

*NOTE: The Unity project that you open is called "client" and it has a local reference to the unity package so if you make changes to the unity package code it will update automatically in the editor.*
