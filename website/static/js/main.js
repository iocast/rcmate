/* rcmate website – theme toggle, navigation, reveal, parallax, GitHub stats */
(function () {
  "use strict";

  var root = document.documentElement;
  var reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)");

  /* ------------------------------------------------------------------ */
  /* Theme toggle                                                        */
  /* ------------------------------------------------------------------ */
  var toggle = document.getElementById("theme-toggle");
  function applyTheme(theme, persist) {
    root.setAttribute("data-theme", theme);
    if (toggle) toggle.setAttribute("aria-pressed", theme === "dark" ? "true" : "false");
    if (persist) { try { localStorage.setItem("theme", theme); } catch (e) { /* ignore */ } }
  }
  applyTheme(root.getAttribute("data-theme") || "dark", false);
  if (toggle) {
    toggle.addEventListener("click", function () {
      applyTheme(root.getAttribute("data-theme") === "dark" ? "light" : "dark", true);
    });
  }
  var systemDark = window.matchMedia("(prefers-color-scheme: dark)");
  var onSystemChange = function (e) {
    var stored = null;
    try { stored = localStorage.getItem("theme"); } catch (err) { /* ignore */ }
    if (!stored) applyTheme(e.matches ? "dark" : "light", false);
  };
  if (systemDark.addEventListener) systemDark.addEventListener("change", onSystemChange);

  /* ------------------------------------------------------------------ */
  /* Navigation: scrolled state + mobile menu                            */
  /* ------------------------------------------------------------------ */
  var nav = document.getElementById("nav");
  var burger = document.getElementById("nav-burger");
  var links = document.getElementById("nav-links");

  function setMenu(open) {
    if (!burger || !links) return;
    burger.setAttribute("aria-expanded", open ? "true" : "false");
    burger.setAttribute("aria-label", open ? "Close menu" : "Open menu");
    links.classList.toggle("is-open", open);
    if (nav) nav.classList.toggle("menu-open", open);
  }
  if (burger) {
    burger.addEventListener("click", function () {
      setMenu(burger.getAttribute("aria-expanded") !== "true");
    });
  }
  if (links) {
    links.addEventListener("click", function (e) {
      if (e.target.closest("a")) setMenu(false);
    });
  }
  document.addEventListener("keydown", function (e) { if (e.key === "Escape") setMenu(false); });

  /* ------------------------------------------------------------------ */
  /* Reveal on scroll                                                    */
  /* ------------------------------------------------------------------ */
  var reveals = document.querySelectorAll(".reveal");
  function finishReveal(el) {
    el.classList.add("is-visible");
    // Drop the reveal classes afterwards so hover transitions are not delayed
    window.setTimeout(function () { el.classList.remove("reveal", "is-visible"); el.style.removeProperty("--i"); }, 1400);
  }
  if ("IntersectionObserver" in window && !reduceMotion.matches) {
    var io = new IntersectionObserver(function (entries) {
      entries.forEach(function (entry) {
        if (entry.isIntersecting) { finishReveal(entry.target); io.unobserve(entry.target); }
      });
    }, { rootMargin: "0px 0px -8% 0px", threshold: 0.08 });
    reveals.forEach(function (el) { io.observe(el); });
  } else {
    reveals.forEach(function (el) { el.classList.add("is-visible"); });
  }

  /* ------------------------------------------------------------------ */
  /* Card spotlight (follows the mouse)                                  */
  /* ------------------------------------------------------------------ */
  document.querySelectorAll(".card").forEach(function (card) {
    card.addEventListener("pointermove", function (e) {
      var r = card.getBoundingClientRect();
      card.style.setProperty("--mx", (e.clientX - r.left) + "px");
      card.style.setProperty("--my", (e.clientY - r.top) + "px");
    });
  });

  /* ------------------------------------------------------------------ */
  /* Parallax                                                            */
  /*  data-depth="0.3"     vertical speed relative to scroll             */
  /*  data-mouse="1"       extra offset following the pointer (hero)     */
  /*  data-parallax-x="x"  horizontal movement while scrolling           */
  /*  data-parallax-scope  element used as reference for the layers      */
  /* ------------------------------------------------------------------ */
  var depthLayers = Array.prototype.slice.call(document.querySelectorAll("[data-depth]"));
  var xLayers = Array.prototype.slice.call(document.querySelectorAll("[data-parallax-x]"));
  var mouseX = 0, mouseY = 0, curX = 0, curY = 0;
  var ticking = false;

  function frame() {
    ticking = false;
    var vh = window.innerHeight;

    // smooth pointer easing
    curX += (mouseX - curX) * 0.08;
    curY += (mouseY - curY) * 0.08;

    for (var i = 0; i < depthLayers.length; i++) {
      var el = depthLayers[i];
      var scope = el.closest("[data-parallax-scope]") || el.parentElement;
      var r = scope.getBoundingClientRect();
      if (r.bottom < -vh * 0.5 || r.top > vh * 1.5) continue;
      var offset = (r.top + r.height / 2) - vh / 2;
      var depth = parseFloat(el.getAttribute("data-depth")) || 0;
      var m = parseFloat(el.getAttribute("data-mouse")) || 0;
      var ty = -offset * depth + curY * m;
      var tx = curX * m;
      el.style.transform = "translate3d(" + tx.toFixed(2) + "px," + ty.toFixed(2) + "px,0)";
    }

    for (var j = 0; j < xLayers.length; j++) {
      var row = xLayers[j];
      var rr = row.getBoundingClientRect();
      if (rr.bottom < -vh || rr.top > vh * 2) continue;
      var c = (rr.top + rr.height / 2) - vh / 2;
      var s = parseFloat(row.getAttribute("data-parallax-x")) || 0;
      row.style.transform = "translate3d(" + (c * s).toFixed(2) + "px,0,0)";
    }

    if (Math.abs(mouseX - curX) > 0.1 || Math.abs(mouseY - curY) > 0.1) requestTick();
  }
  function requestTick() {
    if (!ticking) { ticking = true; window.requestAnimationFrame(frame); }
  }
  function resetParallax() {
    depthLayers.concat(xLayers).forEach(function (el) { el.style.transform = ""; });
  }

  function initParallax() {
    if (reduceMotion.matches) { resetParallax(); return; }
    window.addEventListener("scroll", requestTick, { passive: true });
    window.addEventListener("resize", requestTick);
    var hero = document.getElementById("hero") || document.querySelector(".hero");
    if (hero && window.matchMedia("(pointer: fine)").matches) {
      hero.addEventListener("pointermove", function (e) {
        mouseX = (e.clientX / window.innerWidth - 0.5) * 40;
        mouseY = (e.clientY / window.innerHeight - 0.5) * 40;
        requestTick();
      });
      hero.addEventListener("pointerleave", function () { mouseX = 0; mouseY = 0; requestTick(); });
    }
    requestTick();
  }
  initParallax();
  if (reduceMotion.addEventListener) {
    reduceMotion.addEventListener("change", function () {
      if (reduceMotion.matches) {
        window.removeEventListener("scroll", requestTick);
        resetParallax();
      } else {
        initParallax();
      }
    });
  }

  /* ------------------------------------------------------------------ */
  /* Nav scrolled state                                                  */
  /* ------------------------------------------------------------------ */
  function onScroll() { if (nav) nav.classList.toggle("is-scrolled", window.scrollY > 12); }
  window.addEventListener("scroll", onScroll, { passive: true });
  onScroll();

  /* ------------------------------------------------------------------ */
  /* Copy buttons                                                        */
  /* ------------------------------------------------------------------ */
  function copyText(text, btn) {
    var done = function () {
      var old = btn.textContent;
      btn.textContent = "Copied";
      btn.classList.add("is-copied");
      window.setTimeout(function () { btn.textContent = old; btn.classList.remove("is-copied"); }, 1600);
    };
    if (navigator.clipboard && window.isSecureContext) {
      navigator.clipboard.writeText(text).then(done, function () { /* ignore */ });
    } else {
      var ta = document.createElement("textarea");
      ta.value = text; ta.setAttribute("readonly", ""); ta.style.position = "fixed"; ta.style.opacity = "0";
      document.body.appendChild(ta); ta.select();
      try { document.execCommand("copy"); done(); } catch (e) { /* ignore */ }
      document.body.removeChild(ta);
    }
  }
  document.querySelectorAll("[data-copy]").forEach(function (btn) {
    btn.addEventListener("click", function () {
      var code = btn.parentElement.querySelector("code");
      if (code) copyText(code.textContent.trim(), btn);
    });
  });
  document.querySelectorAll(".prose pre").forEach(function (pre) {
    var btn = document.createElement("button");
    btn.type = "button"; btn.className = "copy-btn"; btn.textContent = "Copy";
    btn.setAttribute("aria-label", "Copy code");
    btn.addEventListener("click", function () {
      var code = pre.querySelector("code");
      copyText((code ? code.textContent : pre.textContent).trim(), btn);
    });
    pre.appendChild(btn);
  });

  /* ------------------------------------------------------------------ */
  /* Footer year                                                         */
  /* ------------------------------------------------------------------ */
  document.querySelectorAll("[data-year]").forEach(function (el) { el.textContent = String(new Date().getFullYear()); });

  /* ------------------------------------------------------------------ */
  /* Live GitHub stats (cached for 1h in sessionStorage)                 */
  /* ------------------------------------------------------------------ */
  var repo = document.body.getAttribute("data-repo");
  if (!repo || !window.fetch) return;

  function fmt(n) {
    if (typeof n !== "number") return "–";
    return n >= 1000 ? (n / 1000).toFixed(n >= 10000 ? 0 : 1).replace(/\.0$/, "") + "k" : String(n);
  }
  function fill(sel, value) {
    document.querySelectorAll(sel).forEach(function (el) { el.textContent = value; });
  }
  function cached(key, url) {
    var now = Date.now();
    try {
      var hit = JSON.parse(sessionStorage.getItem(key) || "null");
      if (hit && now - hit.t < 3600000) return Promise.resolve(hit.d);
    } catch (e) { /* ignore */ }
    return fetch(url, { headers: { Accept: "application/vnd.github+json" } })
      .then(function (r) { return r.ok ? r.json() : null; })
      .then(function (d) {
        if (d) { try { sessionStorage.setItem(key, JSON.stringify({ t: now, d: d })); } catch (e) { /* ignore */ } }
        return d;
      });
  }

  cached("gh-repo-" + repo, "https://api.github.com/repos/" + repo).then(function (d) {
    if (!d) return;
    fill("[data-gh-stars]", fmt(d.stargazers_count));
    fill("[data-gh-forks]", fmt(d.forks_count));
    fill("[data-gh-issues]", fmt(d.open_issues_count));
    fill("[data-gh-lang]", d.language || "–");
  }).catch(function () { /* offline or rate limited */ });

  cached("gh-release-" + repo, "https://api.github.com/repos/" + repo + "/releases/latest").then(function (d) {
    if (d && d.tag_name) fill("[data-gh-release]", d.tag_name + " is out – see what's new");
  }).catch(function () { /* no release yet */ });
})();
