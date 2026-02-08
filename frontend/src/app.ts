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
  // Future stages
  { id: "erosion", label: "Erosion", stage: 2, available: false },
  { id: "rivers", label: "Rivers", stage: 3, available: false },
  { id: "climate", label: "Climate", stage: 4, available: false },
  { id: "biomes", label: "Biomes", stage: 4, available: false },
  { id: "final", label: "Final", stage: 5, available: false },
];

// Elevation parameter IDs for binding sliders
const ELEVATION_PARAMS = [
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
];

class App {
  private activeLayer = "map";
  private layerData = new Map<string, string>();
  private generating = false;

  constructor() {
    this.buildTabs();
    this.bindEvents();
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

    document.getElementById("plates")!.addEventListener("input", (e) => {
      document.getElementById("plates-val")!.textContent = (
        e.target as HTMLInputElement
      ).value;
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
            display.textContent = parseFloat(val) % 1 === 0 ? val : parseFloat(val).toFixed(1);
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

    // Keyboard shortcut: Enter to generate
    document.addEventListener("keydown", (e) => {
      if (e.key === "Enter" && !this.generating) {
        this.generate();
      }
    });
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
    if (data) {
      img.src = data;
      img.style.display = "block";
      placeholder.style.display = "none";
    } else {
      img.style.display = "none";
      placeholder.style.display = "block";
    }
  }

  private readSlider(id: string): number {
    return parseFloat((document.getElementById(id) as HTMLInputElement).value);
  }

  private async generate() {
    if (this.generating) return;
    this.generating = true;

    const btn = document.getElementById("btn-generate") as HTMLButtonElement;
    const status = document.getElementById("status")!;
    btn.disabled = true;
    status.textContent = "generating...";

    const seed =
      parseInt(
        (document.getElementById("seed") as HTMLInputElement).value
      ) || 42;
    const [width, height] = (
      document.getElementById("size") as HTMLSelectElement
    ).value
      .split("x")
      .map(Number);
    const num_plates = parseInt(
      (document.getElementById("plates") as HTMLInputElement).value
    );
    const continental_fraction = parseFloat(
      (document.getElementById("fraction") as HTMLInputElement).value
    );

    const body: Record<string, number> = {
      seed,
      width,
      height,
      num_plates,
      continental_fraction,
    };

    // Include elevation params
    for (const id of ELEVATION_PARAMS) {
      body[id] = this.readSlider(id);
    }

    try {
      const t0 = performance.now();
      const res = await fetch("/api/generate", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });

      if (!res.ok) {
        throw new Error(`${res.status} ${res.statusText}`);
      }

      const data: GenerateResponse = await res.json();
      const roundtrip = performance.now() - t0;

      // Store layer data
      this.layerData.clear();
      for (const layer of data.layers) {
        this.layerData.set(layer.name, layer.data_url);
      }

      this.updateImage();

      // Timings
      const total = data.timings.find((t) => t.name === "TOTAL");
      status.textContent = total
        ? `done — ${total.ms.toFixed(0)}ms gen, ${roundtrip.toFixed(0)}ms total`
        : "done";

      this.updateTimings(data.timings);
    } catch (err) {
      status.textContent = `error: ${err}`;
    } finally {
      btn.disabled = false;
      this.generating = false;
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
