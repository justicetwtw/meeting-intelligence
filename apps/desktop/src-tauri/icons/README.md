# Desktop icon source

`app-icon.svg` is the checked-in, square source of truth for desktop branding.
The generated PNG, ICO, and ICNS assets are committed so a clean clone and tag
release use the project icon without requiring an image tool. Regenerate them
after changing the SVG with:

```text
npm run tauri --workspace @meeting-intelligence/desktop -- icon src-tauri/icons/app-icon.svg
```

The command also refreshes Tauri's mobile/store icon variants. It requires no
runtime dependency or signing credential.
