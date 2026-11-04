# Scene gallery

Every `*.frolic.json` file here is a Frolic scene. To open one, drag it onto the Frolic
window, use **Open…**, or run:

```sh
cargo run --release -- scenes/06-halbach-array.frolic.json
```

The numbered files are the built-in examples. They are compiled into the app and appear
under *Scene → Open example*.

## Sharing your scene

1. Build your scene in Frolic and give it a name and a short description in the
   *Scene* panel. Position the camera the way you'd like people to see it first.
2. Click **Save as…**. The suggested file name comes from the scene name.
3. Open a pull request that adds the file to this folder. Use a descriptive, lowercase,
   hyphenated name, such as `magnetic-levitation-ring.frolic.json`, without a number
   prefix; numbers are reserved for the built-in examples.

Please keep scenes under a few hundred magnets so they open quickly for everyone.

If you'd rather not open a pull request, paste a share code (*Share → Copy share code*)
into an issue or discussion instead.
