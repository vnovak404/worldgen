interface Layer {
  name: string;
  data_url: string;
}

interface TimingEntry {
  name: string;
  ms: number;
}

interface GenerateResponse {
  layers: Layer[];
  timings: TimingEntry[];
  width: number;
  height: number;
}

interface RiversResponse {
  layer: Layer;
  timing: TimingEntry;
}

interface LayerDef {
  id: string;
  label: string;
  stage: number;
  available: boolean;
}

const LAYERS: LayerDef[] = [
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
  { id: "final", label: "Final", stage: 4, available: false },
];

// All parameter IDs (sliders in the Tune panel)
const ELEVATION_PARAMS = [
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
  "river_threshold",
];

// All saveable control IDs (includes top-bar controls)
const ALL_CONTROLS = ["seed", "size", "fraction", ...ELEVATION_PARAMS];

const STORAGE_KEY = "worldgen_params";

class App {
  private activeLayer = "map";
  private layerData = new Map<string, string>();
  private generating = false;
  private riversLoading = false;

  constructor() {
    this.buildTabs();
    this.bindEvents();
    this.loadParams();
  }

  private buildTabs() {
    const bar = document.getElementById("tab-bar")!;
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
        tab.title = `Stage ${def.stage} — not yet implemented`;
        tab.disabled = true;
      }
      bar.appendChild(tab);
    }
  }

  private bindEvents() {
    document
      .getElementById("btn-generate")!
      .addEventListener("click", () => this.generate());

    document.getElementById("btn-random")!.addEventListener("click", () => {
      const input = document.getElementById("seed") as HTMLInputElement;
      input.value = String(Math.floor(Math.random() * 999999));
    });

    document.getElementById("tab-bar")!.addEventListener("click", (e) => {
      const target = e.target as HTMLElement;
      const tab = target.closest(".tab") as HTMLElement | null;
      if (tab && !tab.classList.contains("disabled")) {
        this.setActiveLayer(tab.dataset.layer!);
      }
    });

    document.getElementById("fraction")!.addEventListener("input", (e) => {
      document.getElementById("fraction-val")!.textContent = parseFloat(
        (e.target as HTMLInputElement).value
      ).toFixed(2);
    });

    // Elevation parameter sliders
    for (const id of ELEVATION_PARAMS) {
      const el = document.getElementById(id);
      if (el) {
        el.addEventListener("input", (e) => {
          const val = (e.target as HTMLInputElement).value;
          const display = document.getElementById(`${id}-val`);
          if (display) {
            const num = parseFloat(val);
            display.textContent = num % 1 === 0 ? val : num < 0.1 ? num.toFixed(3) : num.toFixed(1);
          }
        });
      }
    }

    // Toggle elevation controls
    document.getElementById("btn-toggle-elevation")!.addEventListener("click", () => {
      const panel = document.getElementById("elevation-controls")!;
      const btn = document.getElementById("btn-toggle-elevation")!;
      if (panel.style.display === "none") {
        panel.style.display = "flex";
        btn.classList.add("active");
      } else {
        panel.style.display = "none";
        btn.classList.remove("active");
      }
    });

    // Save/Load buttons
    document.getElementById("btn-save")!.addEventListener("click", () => this.saveParams());
    document.getElementById("btn-load")!.addEventListener("click", () => this.loadParams());

    // Keyboard shortcut: Enter to generate
    document.addEventListener("keydown", (e) => {
      if (e.key === "Enter" && !this.generating) {
        this.generate();
      }
    });
  }

  private saveParams() {
    const params: Record<string, string> = {};
    for (const id of ALL_CONTROLS) {
      const el = document.getElementById(id) as HTMLInputElement | HTMLSelectElement | null;
      if (el) params[id] = el.value;
    }
    localStorage.setItem(STORAGE_KEY, JSON.stringify(params));
    const status = document.getElementById("status")!;
    status.textContent = "params saved";
  }

  private loadParams() {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return;
    try {
      const params: Record<string, string> = JSON.parse(raw);
      for (const [id, val] of Object.entries(params)) {
        const el = document.getElementById(id) as HTMLInputElement | HTMLSelectElement | null;
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
    } catch {}
  }

  private setActiveLayer(id: string) {
    this.activeLayer = id;
    document
      .querySelectorAll(".tab")
      .forEach((t) => t.classList.remove("active"));
    document
      .querySelector(`.tab[data-layer="${id}"]`)
      ?.classList.add("active");
    this.updateImage();
  }

  private updateImage() {
    const img = document.getElementById("layer-image") as HTMLImageElement;
    const placeholder = document.getElementById("placeholder")!;
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

  private updateRiversTab() {
    const tab = document.querySelector('.tab[data-layer="rivers"]') as HTMLElement | null;
    if (!tab) return;
    if (this.riversLoading) {
      tab.textContent = "Rivers...";
      tab.classList.add("loading");
    } else {
      tab.textContent = "Rivers";
      tab.classList.remove("loading");
    }
  }

  private readSlider(id: string): number {
    return parseFloat((document.getElementById(id) as HTMLInputElement).value);
  }

  private buildRequestBody(): Record<string, number> {
    const seed =
      parseInt(
        (document.getElementById("seed") as HTMLInputElement).value
      ) || 42;
    const [width, height] = (
      document.getElementById("size") as HTMLSelectElement
    ).value
      .split("x")
      .map(Number);
    const continental_fraction = parseFloat(
      (document.getElementById("fraction") as HTMLInputElement).value
    );

    const body: Record<string, number> = {
      seed,
      width,
      height,
      continental_fraction,
    };

    for (const id of ELEVATION_PARAMS) {
      body[id] = this.readSlider(id);
    }

    return body;
  }

  private async generate() {
    if (this.generating) return;
    this.generating = true;

    const btn = document.getElementById("btn-generate") as HTMLButtonElement;
    const status = document.getElementById("status")!;
    btn.disabled = true;
    status.textContent = "generating...";

    // Clear old rivers data and mark as loading
    this.layerData.delete("rivers");
    this.riversLoading = true;
    this.updateRiversTab();

    const body = this.buildRequestBody();

    try {
      const t0 = performance.now();

      // Fire base generation request
      const baseRes = await fetch("/api/generate", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });

      if (!baseRes.ok) {
        throw new Error(`${baseRes.status} ${baseRes.statusText}`);
      }

      const baseData: GenerateResponse = await baseRes.json();
      const baseRoundtrip = performance.now() - t0;

      // Store base layer data immediately
      this.layerData.clear();
      for (const layer of baseData.layers) {
        this.layerData.set(layer.name, layer.data_url);
      }

      this.updateImage();
      this.saveParams();

      // Show base timings
      const baseTotal = baseData.timings.find((t) => t.name === "TOTAL");
      status.textContent = baseTotal
        ? `done — ${baseTotal.ms.toFixed(0)}ms gen, ${baseRoundtrip.toFixed(0)}ms total | rivers computing...`
        : "done | rivers computing...";

      this.updateTimings(baseData.timings);

      // Enable generate button immediately — user can interact while rivers compute
      btn.disabled = false;
      this.generating = false;

      // Fire rivers request in background
      this.fetchRivers(body, baseData.timings, performance.now());
    } catch (err) {
      status.textContent = `error: ${err}`;
      btn.disabled = false;
      this.generating = false;
      this.riversLoading = false;
      this.updateRiversTab();
    }
  }

  private async fetchRivers(
    _body: Record<string, number>,
    baseTimings: TimingEntry[],
    t0: number
  ) {
    const status = document.getElementById("status")!;
    try {
      const res = await fetch("/api/rivers", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: "{}",
      });

      if (!res.ok) {
        throw new Error(`rivers: ${res.status}`);
      }

      const data: RiversResponse | null = await res.json();
      if (data) {
        this.layerData.set(data.layer.name, data.layer.data_url);

        // Update timings to include hydrology
        const allTimings = [...baseTimings.filter(t => t.name !== "TOTAL"), data.timing];
        const totalMs = allTimings.reduce((s, t) => s + t.ms, 0);
        allTimings.push({ name: "TOTAL", ms: totalMs });

        const roundtrip = performance.now() - t0;
        status.textContent = `done — ${totalMs.toFixed(0)}ms gen, ${roundtrip.toFixed(0)}ms total`;
        this.updateTimings(allTimings);
      }
    } catch (err) {
      status.textContent += ` | rivers error: ${err}`;
    } finally {
      this.riversLoading = false;
      this.updateRiversTab();
      // If user is viewing rivers tab, update the image now
      if (this.activeLayer === "rivers") {
        this.updateImage();
      }
    }
  }

  private updateTimings(timings: TimingEntry[]) {
    const el = document.getElementById("timings")!;
    el.innerHTML = timings
      .filter((t) => t.name !== "TOTAL")
      .map(
        (t) =>
          `<span class="timing"><span class="timing-name">${t.name}</span> <span class="timing-ms">${t.ms.toFixed(1)}</span></span>`
      )
      .join(" ");
  }
}

new App();
