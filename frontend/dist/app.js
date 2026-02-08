"use strict";
(() => {
  // src/app.ts
  var LAYERS = [
    { id: "plates", label: "Plates", stage: 1, available: true },
    { id: "boundaries", label: "Boundaries", stage: 1, available: true },
    { id: "distance", label: "Distance", stage: 1, available: true },
    { id: "heightmap", label: "Heightmap", stage: 1, available: true },
    { id: "map", label: "Map", stage: 1, available: true },
    // Stage 2
    { id: "temperature", label: "Temperature", stage: 2, available: true },
    { id: "precipitation", label: "Precipitation", stage: 2, available: true },
    { id: "rivers", label: "Rivers", stage: 2, available: true },
    // Future stages
    { id: "biomes", label: "Biomes", stage: 3, available: false },
    { id: "final", label: "Final", stage: 4, available: false }
  ];
  var ELEVATION_PARAMS = [
    "num_macroplates",
    "num_microplates",
    "boundary_noise",
    "blur_sigma",
    "mountain_scale",
    "trench_scale",
    "mountain_width",
    "coast_amp",
    "interior_amp",
    "detail_amp",
    "shelf_width",
    "ridge_height",
    "rift_depth",
    "rainfall_scale",
    "river_threshold"
  ];
  var ALL_CONTROLS = ["seed", "size", "fraction", ...ELEVATION_PARAMS];
  var STORAGE_KEY = "worldgen_params";
  var App = class {
    constructor() {
      this.activeLayer = "map";
      this.layerData = /* @__PURE__ */ new Map();
      this.generating = false;
      this.riversLoading = false;
      this.buildTabs();
      this.bindEvents();
      this.loadParams();
    }
    buildTabs() {
      const bar = document.getElementById("tab-bar");
      for (const def of LAYERS) {
        const tab = document.createElement("button");
        tab.className = `tab${def.id === this.activeLayer ? " active" : ""}${!def.available ? " disabled" : ""}`;
        tab.dataset.layer = def.id;
        tab.textContent = def.label;
        if (!def.available) {
          const badge = document.createElement("span");
          badge.className = "stage-badge";
          badge.textContent = `s${def.stage}`;
          tab.appendChild(badge);
          tab.title = `Stage ${def.stage} \u2014 not yet implemented`;
          tab.disabled = true;
        }
        bar.appendChild(tab);
      }
    }
    bindEvents() {
      document.getElementById("btn-generate").addEventListener("click", () => this.generate());
      document.getElementById("btn-random").addEventListener("click", () => {
        const input = document.getElementById("seed");
        input.value = String(Math.floor(Math.random() * 999999));
      });
      document.getElementById("tab-bar").addEventListener("click", (e) => {
        const target = e.target;
        const tab = target.closest(".tab");
        if (tab && !tab.classList.contains("disabled")) {
          this.setActiveLayer(tab.dataset.layer);
        }
      });
      document.getElementById("fraction").addEventListener("input", (e) => {
        document.getElementById("fraction-val").textContent = parseFloat(
          e.target.value
        ).toFixed(2);
      });
      for (const id of ELEVATION_PARAMS) {
        const el = document.getElementById(id);
        if (el) {
          el.addEventListener("input", (e) => {
            const val = e.target.value;
            const display = document.getElementById(`${id}-val`);
            if (display) {
              const num = parseFloat(val);
              display.textContent = num % 1 === 0 ? val : num < 0.1 ? num.toFixed(3) : num.toFixed(1);
            }
          });
        }
      }
      document.getElementById("btn-toggle-elevation").addEventListener("click", () => {
        const panel = document.getElementById("elevation-controls");
        const btn = document.getElementById("btn-toggle-elevation");
        if (panel.style.display === "none") {
          panel.style.display = "flex";
          btn.classList.add("active");
        } else {
          panel.style.display = "none";
          btn.classList.remove("active");
        }
      });
      document.getElementById("btn-save").addEventListener("click", () => this.saveParams());
      document.getElementById("btn-load").addEventListener("click", () => this.loadParams());
      document.addEventListener("keydown", (e) => {
        if (e.key === "Enter" && !this.generating) {
          this.generate();
        }
      });
    }
    saveParams() {
      const params = {};
      for (const id of ALL_CONTROLS) {
        const el = document.getElementById(id);
        if (el) params[id] = el.value;
      }
      localStorage.setItem(STORAGE_KEY, JSON.stringify(params));
      const status = document.getElementById("status");
      status.textContent = "params saved";
    }
    loadParams() {
      const raw = localStorage.getItem(STORAGE_KEY);
      if (!raw) return;
      try {
        const params = JSON.parse(raw);
        for (const [id, val] of Object.entries(params)) {
          const el = document.getElementById(id);
          if (!el) continue;
          el.value = val;
          const display = document.getElementById(`${id}-val`);
          if (display) {
            const num = parseFloat(val);
            if (id === "fraction") {
              display.textContent = num.toFixed(2);
            } else {
              display.textContent = num % 1 === 0 ? val : num < 0.1 ? num.toFixed(3) : num.toFixed(1);
            }
          }
        }
      } catch {
      }
    }
    setActiveLayer(id) {
      this.activeLayer = id;
      document.querySelectorAll(".tab").forEach((t) => t.classList.remove("active"));
      document.querySelector(`.tab[data-layer="${id}"]`)?.classList.add("active");
      this.updateImage();
    }
    updateImage() {
      const img = document.getElementById("layer-image");
      const placeholder = document.getElementById("placeholder");
      const data = this.layerData.get(this.activeLayer);
      if (this.activeLayer === "rivers" && this.riversLoading && !data) {
        img.style.display = "none";
        placeholder.style.display = "block";
        placeholder.textContent = "computing rivers...";
        return;
      }
      if (data) {
        img.src = data;
        img.style.display = "block";
        placeholder.style.display = "none";
      } else {
        img.style.display = "none";
        placeholder.style.display = "block";
        placeholder.textContent = "press Generate to start";
      }
    }
    updateRiversTab() {
      const tab = document.querySelector('.tab[data-layer="rivers"]');
      if (!tab) return;
      if (this.riversLoading) {
        tab.textContent = "Rivers...";
        tab.classList.add("loading");
      } else {
        tab.textContent = "Rivers";
        tab.classList.remove("loading");
      }
    }
    readSlider(id) {
      return parseFloat(document.getElementById(id).value);
    }
    buildRequestBody() {
      const seed = parseInt(
        document.getElementById("seed").value
      ) || 42;
      const [width, height] = document.getElementById("size").value.split("x").map(Number);
      const continental_fraction = parseFloat(
        document.getElementById("fraction").value
      );
      const body = {
        seed,
        width,
        height,
        continental_fraction
      };
      for (const id of ELEVATION_PARAMS) {
        body[id] = this.readSlider(id);
      }
      return body;
    }
    async generate() {
      if (this.generating) return;
      this.generating = true;
      const btn = document.getElementById("btn-generate");
      const status = document.getElementById("status");
      btn.disabled = true;
      status.textContent = "generating...";
      this.layerData.delete("rivers");
      this.riversLoading = true;
      this.updateRiversTab();
      const body = this.buildRequestBody();
      try {
        const t0 = performance.now();
        const baseRes = await fetch("/api/generate", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(body)
        });
        if (!baseRes.ok) {
          throw new Error(`${baseRes.status} ${baseRes.statusText}`);
        }
        const baseData = await baseRes.json();
        const baseRoundtrip = performance.now() - t0;
        this.layerData.clear();
        for (const layer of baseData.layers) {
          this.layerData.set(layer.name, layer.data_url);
        }
        this.updateImage();
        this.saveParams();
        const baseTotal = baseData.timings.find((t) => t.name === "TOTAL");
        status.textContent = baseTotal ? `done \u2014 ${baseTotal.ms.toFixed(0)}ms gen, ${baseRoundtrip.toFixed(0)}ms total | rivers computing...` : "done | rivers computing...";
        this.updateTimings(baseData.timings);
        btn.disabled = false;
        this.generating = false;
        this.fetchRivers(body, baseData.timings, performance.now());
      } catch (err) {
        status.textContent = `error: ${err}`;
        btn.disabled = false;
        this.generating = false;
        this.riversLoading = false;
        this.updateRiversTab();
      }
    }
    async fetchRivers(_body, baseTimings, t0) {
      const status = document.getElementById("status");
      try {
        const res = await fetch("/api/rivers", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: "{}"
        });
        if (!res.ok) {
          throw new Error(`rivers: ${res.status}`);
        }
        const data = await res.json();
        if (data) {
          this.layerData.set(data.layer.name, data.layer.data_url);
          const allTimings = [...baseTimings.filter((t) => t.name !== "TOTAL"), data.timing];
          const totalMs = allTimings.reduce((s, t) => s + t.ms, 0);
          allTimings.push({ name: "TOTAL", ms: totalMs });
          const roundtrip = performance.now() - t0;
          status.textContent = `done \u2014 ${totalMs.toFixed(0)}ms gen, ${roundtrip.toFixed(0)}ms total`;
          this.updateTimings(allTimings);
        }
      } catch (err) {
        status.textContent += ` | rivers error: ${err}`;
      } finally {
        this.riversLoading = false;
        this.updateRiversTab();
        if (this.activeLayer === "rivers") {
          this.updateImage();
        }
      }
    }
    updateTimings(timings) {
      const el = document.getElementById("timings");
      el.innerHTML = timings.filter((t) => t.name !== "TOTAL").map(
        (t) => `<span class="timing"><span class="timing-name">${t.name}</span> <span class="timing-ms">${t.ms.toFixed(1)}</span></span>`
      ).join(" ");
    }
  };
  new App();
})();
