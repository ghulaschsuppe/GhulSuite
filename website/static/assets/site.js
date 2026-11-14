// SPDX-License-Identifier: AGPL-3.0-or-later
// GhulSuite website: effects, the notice and the language choice. Nothing is sent anywhere;
// the only things kept are two entries in the browser's local storage (see the privacy page).
(() => {
  "use strict";
  const doc = document.documentElement;
  doc.classList.remove("no-js");
  const calm = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const store = {
    get(k) { try { return window.localStorage.getItem(k); } catch (e) { return null; } },
    set(k, v) { try { window.localStorage.setItem(k, v); } catch (e) { /* private mode: fine */ } },
  };

  // Language: on the first visit of the German start page, readers whose browser is not set to
  // German go to the English page once. A choice made with the switch is remembered.
  const lang = doc.lang;
  document.querySelectorAll("[data-lang]").forEach((a) => {
    a.addEventListener("click", () => store.set("gs-lang", a.dataset.lang));
  });
  if (doc.dataset.page === "home" && lang === "de" && !store.get("gs-lang")) {
    const wanted = (navigator.languages && navigator.languages[0]) || navigator.language || "de";
    if (!/^de\b/i.test(wanted)) {
      store.set("gs-lang", "en");
      window.location.replace("en/" + window.location.hash);
      return;
    }
  }

  // Header: a line below it once the page scrolls; the menu on small screens.
  const top = document.querySelector(".top");
  const onScroll = () => top && top.classList.toggle("scrolled", window.scrollY > 8);
  window.addEventListener("scroll", onScroll, { passive: true });
  onScroll();
  const menuBtn = document.querySelector(".menu-btn");
  const nav = document.querySelector(".nav");
  if (menuBtn && nav) {
    menuBtn.addEventListener("click", () => {
      const open = nav.classList.toggle("open");
      menuBtn.setAttribute("aria-expanded", String(open));
    });
    nav.querySelectorAll("a[href^='#']").forEach((a) => a.addEventListener("click", () => {
      nav.classList.remove("open");
      menuBtn.setAttribute("aria-expanded", "false");
    }));
  }

  // Reveal on scroll.
  const items = document.querySelectorAll(".rv");
  if ("IntersectionObserver" in window && !calm) {
    const io = new IntersectionObserver((entries) => {
      entries.forEach((e) => {
        if (e.isIntersecting) { e.target.classList.add("in"); io.unobserve(e.target); }
      });
    }, { rootMargin: "0px 0px -8% 0px", threshold: 0.08 });
    items.forEach((el) => io.observe(el));
  } else {
    items.forEach((el) => el.classList.add("in"));
  }

  // Spotlight on cards.
  document.querySelectorAll(".card").forEach((card) => {
    card.addEventListener("pointermove", (e) => {
      const r = card.getBoundingClientRect();
      card.style.setProperty("--mx", `${e.clientX - r.left}px`);
      card.style.setProperty("--my", `${e.clientY - r.top}px`);
    });
  });

  // The devices in the hero lean towards the pointer.
  const stage = document.querySelector(".stage");
  if (stage && !calm && window.matchMedia("(pointer: fine)").matches) {
    const devices = stage.querySelectorAll(".device");
    let frame = 0;
    window.addEventListener("pointermove", (e) => {
      if (frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        const x = e.clientX / window.innerWidth - 0.5;
        const y = e.clientY / window.innerHeight - 0.5;
        devices.forEach((d, i) => {
          const k = i === 0 ? 12 : 8;
          d.style.transform = `rotateY(${x * k}deg) rotateX(${-y * k}deg)`;
        });
      });
    }, { passive: true });
  }

  // The word in the headline turns into ciphertext and back into the next word.
  const word = document.querySelector("[data-words]");
  if (word) {
    let list = [];
    try { list = JSON.parse(word.dataset.words); } catch (e) { list = []; }
    const glyphs = "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz0123456789+/=#%&";
    let i = 0;
    const show = (target, color) => {
      word.style.color = color;
      if (calm) { word.textContent = target; return; }
      const from = word.textContent;
      const len = Math.max(from.length, target.length);
      let step = 0;
      const steps = 22;
      const tick = () => {
        step++;
        let out = "";
        for (let n = 0; n < len; n++) {
          const done = step / steps > n / len + 0.15;
          if (done) out += target[n] || "";
          else out += glyphs[(Math.random() * glyphs.length) | 0];
        }
        word.textContent = out;
        if (step < steps + 6) requestAnimationFrame(() => setTimeout(tick, 28));
        else word.textContent = target;
      };
      tick();
    };
    if (list.length > 1) {
      setInterval(() => {
        i = (i + 1) % list.length;
        show(list[i].w, list[i].c);
      }, 2600);
    }
  }

  // Counters count up once they are seen.
  const counters = document.querySelectorAll("[data-count]");
  const fmt = new Intl.NumberFormat(lang === "de" ? "de-DE" : "en-US");
  const count = (el) => {
    const end = Number(el.dataset.count);
    const suffix = el.dataset.suffix || "";
    if (calm) { el.textContent = fmt.format(end) + suffix; return; }
    const t0 = performance.now();
    const dur = 1600;
    const run = (t) => {
      const p = Math.min(1, (t - t0) / dur);
      const v = Math.round(end * (1 - Math.pow(1 - p, 3)));
      el.textContent = fmt.format(v) + suffix;
      if (p < 1) requestAnimationFrame(run);
    };
    requestAnimationFrame(run);
  };
  if ("IntersectionObserver" in window) {
    const co = new IntersectionObserver((entries) => {
      entries.forEach((e) => { if (e.isIntersecting) { count(e.target); co.unobserve(e.target); } });
    }, { threshold: 0.4 });
    counters.forEach((el) => co.observe(el));
  } else {
    counters.forEach(count);
  }

  // Gallery: drag with the mouse.
  document.querySelectorAll(".gallery").forEach((g) => {
    let down = false, startX = 0, startLeft = 0, moved = false;
    g.addEventListener("pointerdown", (e) => {
      if (e.pointerType !== "mouse") return;
      down = true; moved = false; startX = e.clientX; startLeft = g.scrollLeft;
      g.classList.add("drag");
    });
    window.addEventListener("pointermove", (e) => {
      if (!down) return;
      const dx = e.clientX - startX;
      if (Math.abs(dx) > 3) moved = true;
      g.scrollLeft = startLeft - dx;
    });
    window.addEventListener("pointerup", () => { down = false; g.classList.remove("drag"); });
    g.addEventListener("click", (e) => { if (moved) e.preventDefault(); }, true);
  });

  // The notice: no cookies, no tracking. Shown until it is acknowledged once.
  const notice = document.querySelector(".notice");
  if (notice && !store.get("gs-notice")) {
    setTimeout(() => notice.classList.add("show"), 900);
    const ok = notice.querySelector("[data-ok]");
    if (ok) ok.addEventListener("click", () => {
      store.set("gs-notice", "1");
      notice.classList.remove("show");
    });
  }

  // Year in the footer.
  document.querySelectorAll("[data-year]").forEach((el) => { el.textContent = String(new Date().getFullYear()); });
})();
