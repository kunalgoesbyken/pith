const { app, BrowserWindow } = require("electron");

app.whenReady().then(() => {
  const window = new BrowserWindow({ width: 900, height: 600 });
  window.loadURL(process.argv.at(-1));
});

app.on("window-all-closed", () => app.quit());
