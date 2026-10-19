/* Progressive enhancement. No fetches or dependencies, including on file://. */
(() => {
  "use strict";
  const root = document.documentElement;
  root.classList.add("js");
  const search = document.getElementById("search");
  const kind = document.getElementById("kind-filter");
  const moduleFilter = document.getElementById("module-filter");
  const cards = [...document.querySelectorAll(".api-card")];
  const links = [...document.querySelectorAll("[data-id]")];
  const resultCount = document.getElementById("result-count");
  const announce = (message) => { document.getElementById("announcement").textContent = message; };
  const sidebar = document.getElementById("sidebar");
  const menu = document.getElementById("menu-toggle");
  const theme = document.getElementById("theme-toggle");
  document.getElementById("search-tools").hidden = false;
  menu.hidden = false;
  theme.hidden = false;

  // Storage may be unavailable for local files or blocked browser settings.
  try {
    const saved = localStorage.getItem("adesh-docs-theme");
    if (saved === "dark" || saved === "light") root.dataset.theme = saved;
  } catch (_) { /* System preference remains the default. */ }
  const preference = window.matchMedia("(prefers-color-scheme: light)");
  const currentTheme = () => root.dataset.theme || (preference.matches ? "light" : "dark");
  const themeLabel = () => {
    const next = currentTheme() === "dark" ? "light" : "dark";
    theme.textContent = next === "light" ? "Light" : "Dark";
    theme.setAttribute("aria-label", `Switch to ${next} theme`);
  };
  themeLabel();
  preference.addEventListener?.("change", themeLabel);
  theme.addEventListener("click", () => {
    root.dataset.theme = currentTheme() === "dark" ? "light" : "dark";
    try { localStorage.setItem("adesh-docs-theme", root.dataset.theme); } catch (_) { /* Optional. */ }
    themeLabel();
  });

  function closeMenu() {
    sidebar.classList.remove("is-open");
    menu.setAttribute("aria-expanded", "false");
  }
  menu.addEventListener("click", () => {
    const open = sidebar.classList.toggle("is-open");
    menu.setAttribute("aria-expanded", String(open));
  });
  document.addEventListener("click", (event) => {
    if (!sidebar.contains(event.target) && !menu.contains(event.target)) closeMenu();
  });

  function filter() {
    const terms = search.value.toLocaleLowerCase().trim().split(/\s+/).filter(Boolean);
    let count = 0;
    const visible = new Set();
    cards.forEach((card) => {
      const matches = (!kind.value || card.dataset.kind === kind.value) &&
        (!moduleFilter.value || card.dataset.file === moduleFilter.value) &&
        terms.every((term) => card.dataset.search.includes(term));
      card.hidden = !matches;
      if (matches) { count++; visible.add(card.id); }
    });
    links.forEach((link) => { link.hidden = !visible.has(link.dataset.id); });
    document.querySelectorAll(".nav-group").forEach((group) => {
      group.hidden = ![...group.querySelectorAll(".symbol-link")].some((link) => !link.hidden);
    });
    resultCount.textContent = `${count} of ${cards.length} symbol${cards.length === 1 ? "" : "s"}`;
    document.getElementById("no-results").hidden = count !== 0 || cards.length === 0;
  }
  let searchTimer;
  search.addEventListener("input", () => {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(filter, 100);
  });
  kind.addEventListener("change", filter);
  moduleFilter.addEventListener("change", filter);
  function reset() {
    clearTimeout(searchTimer);
    search.value = ""; kind.value = ""; moduleFilter.value = "";
    filter();
  }
  document.getElementById("clear-filters").addEventListener("click", () => { reset(); search.focus(); });

  function select(id) {
    links.forEach((link) => {
      if (link.dataset.id === id) link.setAttribute("aria-current", "location");
      else link.removeAttribute("aria-current");
    });
  }
  function revealHash() {
    let id;
    try { id = decodeURIComponent(location.hash.slice(1)); } catch (_) { return; }
    const target = document.getElementById(id);
    if (!target) return;
    if (target.classList.contains("api-card") && target.hidden) reset();
    select(id);
    target.scrollIntoView({ block: "start" });
  }
  window.addEventListener("hashchange", revealHash);
  document.querySelectorAll('a[href^="#"]').forEach((link) => {
    link.addEventListener("click", () => {
      closeMenu();
      const id = link.getAttribute("href").slice(1);
      const target = document.getElementById(id);
      if (target?.hidden) reset();
      select(id);
      // A repeated click does not trigger hashchange.
      if (location.hash === `#${id}`) target?.scrollIntoView({ block: "start" });
    });
  });
  if ("IntersectionObserver" in window) {
    const observer = new IntersectionObserver((entries) => {
      const visible = entries.filter((entry) => entry.isIntersecting)
        .sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top);
      if (visible.length) select(visible[0].target.id);
    }, { rootMargin: "-90px 0px -55% 0px", threshold: 0 });
    cards.forEach((card) => observer.observe(card));
  }
  if (location.hash) revealHash();

  document.addEventListener("keydown", (event) => {
    const editing = event.target instanceof HTMLElement &&
      (event.target.matches("input, textarea, select") || event.target.isContentEditable);
    if (((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") ||
      (!editing && !event.ctrlKey && !event.metaKey && !event.altKey && event.key === "/")) {
      event.preventDefault(); search.focus(); search.select();
    }
    if (event.key === "Escape") {
      if (sidebar.classList.contains("is-open")) { closeMenu(); menu.focus(); }
      else if (document.activeElement === search) { reset(); search.blur(); }
    }
  });

  async function copyText(value) {
    if (navigator.clipboard?.writeText) {
      try { await navigator.clipboard.writeText(value); return; } catch (_) { /* Try local-file fallback. */ }
    }
    const previous = document.activeElement;
    const area = document.createElement("textarea");
    area.value = value;
    area.setAttribute("aria-label", "Code to copy");
    area.style.cssText = "position:fixed;left:-9999px;top:0";
    document.body.append(area);
    area.select();
    let copied = false;
    try { copied = document.execCommand("copy"); }
    finally { area.remove(); previous?.focus(); }
    if (!copied) throw new Error("Clipboard unavailable");
  }
  document.querySelectorAll(".copy-code").forEach((button) => {
    button.addEventListener("click", async () => {
      const code = button.closest(".code-block").querySelector("code");
      try {
        await copyText(code.textContent);
        button.textContent = "Copied";
        announce("Code copied to clipboard.");
      } catch (_) {
        button.textContent = "Select code";
        const range = document.createRange();
        range.selectNodeContents(code);
        const selection = window.getSelection();
        selection.removeAllRanges(); selection.addRange(range);
        announce("Clipboard unavailable. Code selected for manual copying.");
      }
      setTimeout(() => { button.textContent = "Copy"; }, 2200);
    });
  });
})();
