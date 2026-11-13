(() => {
  const system = matchMedia("(prefers-color-scheme: dark)");
  const key = "boardcast-theme";
  function storedTheme() {
    try {
      const value = sessionStorage.getItem(key);
      return value === "light" || value === "dark" ? value : null;
    } catch { return null; }
  }
  let choice = storedTheme();
  function update() {
    if (choice) document.documentElement.dataset.theme = choice;
    else delete document.documentElement.dataset.theme;
    const current = choice || (system.matches ? "dark" : "light");
    const button = document.querySelector("#theme-toggle");
    if (button) {
      const next = current === "dark" ? "light" : "dark";
      const followsSystem = next === (system.matches ? "dark" : "light");
      button.title = `Switch to ${next} theme${followsSystem ? " (system)" : ""}`;
      button.setAttribute("aria-label", button.title);
    }
    const meta = document.querySelector('meta[name="theme-color"]');
    if (meta) meta.content = getComputedStyle(document.documentElement).getPropertyValue("--bg").trim();
    window.dispatchEvent(new Event("boardcast-theme-change"));
    // Keep the demo in the first page in sync even when browser storage is blocked.
    for (const frame of document.querySelectorAll(".try iframe, iframe.hero-board")) frame.contentWindow?.dispatchEvent(new CustomEvent("boardcast-theme-choice", { detail: choice }));
  }
  update();
  document.addEventListener("DOMContentLoaded", () => {
    update();
    for (const frame of document.querySelectorAll(".try iframe, iframe.hero-board")) frame.addEventListener("load", update);
    document.querySelector("#theme-toggle")?.addEventListener("click", () => {
      const automatic = system.matches ? "dark" : "light";
      const next = (choice || automatic) === "dark" ? "light" : "dark";
      choice = next === automatic ? null : next;
      try {
        if (choice) sessionStorage.setItem(key, choice);
        else sessionStorage.removeItem(key);
      } catch { /* The button still works when browser storage is unavailable. */ }
      update();
    });
  });
  system.addEventListener("change", update);
  window.addEventListener("boardcast-theme-choice", event => {
    choice = event.detail === "light" || event.detail === "dark" ? event.detail : null;
    update();
  });
})();
