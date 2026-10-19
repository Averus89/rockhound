// Rockhound site: overlay demo, signature lookup and scroll reveals.
"use strict";

// Copy of data/signatures.json (data_version 2026.10.19, SC 4.10.2), kept
// inline so the site has no build step. Refresh it when the table changes.
// [name, category, tier, base, max_count]
const SOURCES = [
  ["Quantainium", "ship_mining", "legendary", 3170, 100],
  ["Stileron", "ship_mining", "legendary", 3185, 100],
  ["Savrilium", "ship_mining", "legendary", 3200, 100],
  ["Ouratite", "ship_mining", "epic", 3370, 100],
  ["Riccite", "ship_mining", "epic", 3385, 100],
  ["Lindinium", "ship_mining", "epic", 3400, 100],
  ["Beryl", "ship_mining", "rare", 3540, 100],
  ["Taranite", "ship_mining", "rare", 3555, 100],
  ["Borase", "ship_mining", "rare", 3570, 100],
  ["Gold", "ship_mining", "rare", 3585, 100],
  ["Bexalite", "ship_mining", "rare", 3600, 100],
  ["Laranite", "ship_mining", "uncommon", 3825, 100],
  ["Aslarite", "ship_mining", "uncommon", 3840, 100],
  ["Titanium", "ship_mining", "uncommon", 3855, 100],
  ["Tungsten", "ship_mining", "uncommon", 3870, 100],
  ["Agricium", "ship_mining", "uncommon", 3885, 100],
  ["Torite", "ship_mining", "uncommon", 3900, 100],
  ["Hephaestanite", "ship_mining", "common", 4180, 100],
  ["Tin", "ship_mining", "common", 4195, 100],
  ["Quartz", "ship_mining", "common", 4210, 100],
  ["Corundum", "ship_mining", "common", 4225, 100],
  ["Copper", "ship_mining", "common", 4240, 100],
  ["Silicon", "ship_mining", "common", 4255, 100],
  ["Iron", "ship_mining", "common", 4270, 100],
  ["Aluminum", "ship_mining", "common", 4285, 100],
  ["Ice", "ship_mining", "common", 4300, 100],
  ["Small Ground Deposit", "ground_deposit", null, 3000, 50],
  ["Large Ground Deposit", "ground_deposit", null, 4000, 30],
  ["Salvage Panels", "salvage", null, 2000, 100],
  ["Small Wreck Debris", "salvage", null, 1700, 20],
  ["Medium Wreck Debris", "salvage", null, 1850, 20],
  ["Large Wreck Debris", "salvage", null, 2400, 20],
  ["Capital Wreck Debris", "salvage", null, 3000, 20],
];

const CATEGORY = {
  ship_mining: { label: "Ship mining", where: "Space or planet", badge: null },
  ground_deposit: { label: "Ground deposit", where: "On a planet", badge: "ground" },
  salvage: { label: "Salvage", where: "In space", badge: "salvage" },
};

const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
const fmt = new Intl.NumberFormat("en-US");

/** Every source whose base times a whole rock count gives this signature. */
function matchesFor(value) {
  const found = [];
  for (const [name, category, tier, base, max] of SOURCES) {
    const count = value / base;
    if (Number.isInteger(count) && count >= 1 && count <= max) {
      found.push({ name, category, tier, base, count });
    }
  }
  return found;
}

function tierBadge(tier, category) {
  const span = document.createElement("span");
  span.className = "tier";
  span.dataset.tier = tier ?? CATEGORY[category].badge;
  span.textContent = tier ?? CATEGORY[category].label;
  return span;
}

// Hero overlay: cycles through a few real readings.
function startDemo() {
  const readings = [17200, 9510, 13600, 10755, 5550];
  const overlay = document.getElementById("demo");
  const value = document.getElementById("demo-value");
  const name = document.getElementById("demo-name");
  const tier = document.getElementById("demo-tier");
  const kind = document.getElementById("demo-kind");
  const ms = document.getElementById("demo-ms");
  if (!overlay || reduceMotion) return;

  let i = 0;
  setInterval(() => {
    if (document.hidden) return;
    i = (i + 1) % readings.length;
    const best = matchesFor(readings[i])[0];
    value.textContent = fmt.format(readings[i]);
    name.textContent = `${best.name} ×${best.count}`;
    tier.replaceWith(Object.assign(tierBadge(best.tier, best.category), { id: "demo-tier" }));
    kind.textContent = CATEGORY[best.category].label;
    ms.textContent = String(2 + Math.floor(Math.random() * 3));
    overlay.classList.remove("swap");
    void overlay.offsetWidth; // restart the animation
    overlay.classList.add("swap");
  }, 3200);
}

// Signature lookup form.
function startLookup() {
  const form = document.getElementById("lookup-form");
  const input = document.getElementById("lookup-input");
  const list = document.getElementById("lookup-matches");
  const note = document.getElementById("lookup-note");
  if (!form) return;

  function render() {
    const value = Number(input.value.replace(/[^\d]/g, ""));
    list.replaceChildren();
    note.textContent = "";
    if (!value) {
      list.append(emptyRow("Type the number shown next to the pin, for example 17,200."));
      return;
    }
    const found = matchesFor(value);
    if (found.length === 0) {
      list.append(emptyRow(`Nothing in the table makes ${fmt.format(value)}. Check the digits, or ping again.`));
      return;
    }
    found.forEach((m, i) => {
      const li = document.createElement("li");
      li.className = "match";
      li.style.animationDelay = `${i * 60}ms`;
      const nameEl = document.createElement("span");
      nameEl.className = "match-name";
      nameEl.textContent = `${m.name} ×${m.count}`;
      const math = document.createElement("span");
      math.className = "match-math";
      math.textContent = `${fmt.format(m.base)} × ${m.count} · ${CATEGORY[m.category].label} · ${CATEGORY[m.category].where}`;
      li.append(nameEl, tierBadge(m.tier, m.category), math);
      list.append(li);
    });
    if (found.length > 1) {
      note.textContent = "More than one source makes this number. In the app, setting where you are puts the likely one first.";
    }
  }

  function emptyRow(text) {
    const li = document.createElement("li");
    li.className = "match-empty";
    li.textContent = text;
    return li;
  }

  form.addEventListener("submit", (e) => e.preventDefault());
  input.addEventListener("input", render);
  render();
}

// Plays the arrive motion on elements as they scroll in. They stay visible
// without it, so nothing is hidden if the observer never fires.
function startReveals() {
  if (reduceMotion || !("IntersectionObserver" in window)) return;
  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting) {
          entry.target.classList.add("in");
          observer.unobserve(entry.target);
        }
      }
    },
    { threshold: 0 },
  );
  for (const el of document.querySelectorAll(".reveal")) {
    const box = el.getBoundingClientRect();
    if (box.top > window.innerHeight) observer.observe(el);
  }
}

startDemo();
startLookup();
startReveals();
