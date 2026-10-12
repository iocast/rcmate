/* Runs synchronously in <head> to apply the theme before first paint. */
(function () {
  var root = document.documentElement;
  root.classList.add("js");
  var stored = null;
  try { stored = localStorage.getItem("theme"); } catch (e) { /* storage blocked */ }
  var prefersDark = window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches;
  root.setAttribute("data-theme", stored === "light" || stored === "dark" ? stored : (prefersDark ? "dark" : "light"));
})();
