# UMP Dash scrum demo

A dependency-free HTML presentation for the UMP Dash scrum demo.

## Preview

Open `index.html` directly in a browser, or serve the directory locally:

```bash
python3 -m http.server 4173 --directory presentation
```

Then visit <http://localhost:4173>.

## Controls

- `→`, `Space`, or `Page Down`: next slide
- `←` or `Page Up`: previous slide
- `Home` / `End`: first / last slide
- `F`: enter or leave fullscreen
- `?`: keyboard help
- Swipe left or right on touch screens

The slide number is reflected in the URL hash, so a link such as `index.html#4`
opens directly on slide four. Browser printing exports each slide as a separate
page.

## Edit

- `index.html` contains the story and slide markup.
- `styles.css` contains the visual system and transitions.
- `deck.js` contains navigation, fullscreen, URL state, and touch controls.
