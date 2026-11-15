# Basic sample

- `button.zur`: a button driven by a state machine (hover, press, click) with a view model
  (`hover`, `pressed`, `clicks`, `accent`, `pulse`) and a `clicked` event.
- `hello.zur`: a looping animation.
- `ZuriButtonSample.cs`: builds a canvas at runtime, plays the button and recolors it on every click.

## Try the button

1. Create an empty GameObject and add **Zuri Button Sample**.
2. Assign `button.zur` to its **Button** field.
3. Add **GameObject > UI > Event System**.
4. Press Play. Hover and click the button.

## Try the animation

1. Add **GameObject > UI > Raw Image** and set its size to 400 x 300.
2. Add **Zuri > Zuri Player** and **Zuri > Zuri Raw Image** to it.
3. Assign `hello.zur` to the player's **Asset** field. The first frame appears; press Play to animate.

Both files are compiled from `examples/*.json` in the Zuri repository by `scripts/build-unity-plugins.sh`.
