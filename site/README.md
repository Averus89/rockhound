# Rockhound website

One static page, no build step. Open `index.html` in a browser, or serve the
folder locally with `npx serve site`.

`.github/workflows/pages.yml` publishes this folder to GitHub Pages whenever
`site/` changes on main (or by hand from the Actions tab). Pages has to be
switched on once: Settings > Pages > Source: GitHub Actions. The site is then
at https://averus89.github.io/rockhound/.

- `tokens.css`: brand tokens, a copy of the promo kit's `rockhound-tokens.css`.
- `style.css`: layout and components, built only from those tokens.
- `main.js`: the hero overlay demo, the "Try a number" lookup and scroll
  reveals. It carries an inline copy of `data/signatures.json`; refresh it
  when the table changes.

Animation uses the app's "arrive" motion and turns off under
`prefers-reduced-motion`. Download links point at the latest GitHub Release.
