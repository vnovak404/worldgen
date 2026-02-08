"use strict";
(() => {
  // src/app.ts
  var LAYERS = [
    { id: "plates", label: "Plates", stage: 1, available: true },
    { id: "boundaries", label: "Boundaries", stage: 1, available: true },
    { id: "distance", label: "Distance", stage: 1, available: true },
    { id: "heightmap", label: "Heightmap", stage: 1, available: true },
    { id: "map", label: "Map", stage: 1, available: true },
    // Future stages
    { id: "erosion", label: "Erosion", stage: 2, available: false },
    { id: "rivers", label: "Rivers", stage: 3, available: false },
    { id: "climate", label: "Climate", stage: 4, available: false },
    { id: "biomes", label: "Biomes", stage: 4, available: false },
    { id: "final", label: "Final", stage: 5, available: false }
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
    "rift_depth"
  ];
  var App = class {
    constructor() {
      this.activeLayer = "map";
      this.layerData = /* @__PURE__ */ new Map();
      this.generating = false;
      this.buildTabs();
      this.bindEvents();
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
              display.textContent = parseFloat(val) % 1 === 0 ? val : parseFloat(val).toFixed(1);
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
      document.addEventListener("keydown", (e) => {
        if (e.key === "Enter" && !this.generating) {
          this.generate();
        }
      });
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
      if (data) {
        img.src = data;
        img.style.display = "block";
        placeholder.style.display = "none";
      } else {
        img.style.display = "none";
        placeholder.style.display = "block";
      }
    }
    readSlider(id) {
      return parseFloat(document.getElementById(id).value);
    }
    async generate() {
      if (this.generating) return;
      this.generating = true;
      const btn = document.getElementById("btn-generate");
      const status = document.getElementById("status");
      btn.disabled = true;
      status.textContent = "generating...";
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
      try {
        const t0 = performance.now();
        const res = await fetch("/api/generate", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(body)
        });
        if (!res.ok) {
          throw new Error(`${res.status} ${res.statusText}`);
        }
        const data = await res.json();
        const roundtrip = performance.now() - t0;
        this.layerData.clear();
        for (const layer of data.layers) {
          this.layerData.set(layer.name, layer.data_url);
        }
        this.updateImage();
        const total = data.timings.find((t) => t.name === "TOTAL");
        status.textContent = total ? `done \u2014 ${total.ms.toFixed(0)}ms gen, ${roundtrip.toFixed(0)}ms total` : "done";
        this.updateTimings(data.timings);
      } catch (err) {
        status.textContent = `error: ${err}`;
      } finally {
        btn.disabled = false;
        this.generating = false;
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
