import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Check,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Eye,
  EyeOff,
  FileImage,
  FolderOpen,
  ImagePlus,
  Layers3,
  Minus,
  Plus,
  RefreshCw,
  Save,
  SlidersHorizontal,
  Undo2,
  Redo2,
  Columns2,
  Copy,
  Video,
  FolderSync,
  Maximize2,
  X,
  ZoomIn,
  ZoomOut,
} from "lucide-react";
import { ChangeEvent, DragEvent, useEffect, useMemo, useRef, useState } from "react";
import brandLogo from "../../assets/dotwise_icon.png";

type Layer = { type: string; enabled: boolean; opacity: number; [key: string]: any };
type Pipeline = { layers: Layer[]; animate_noise?: boolean; composite?: Record<string, any> };
type VideoDocument = { path: string; duration: number; fps: number; width: number; height: number };
type DocumentTab = {
  id: number;
  name: string;
  source: string | null;
  previewSource: string | null;
  previewId: number | null;
  preview: string | null;
  pipeline: Pipeline;
  history: Pipeline[];
  historyIndex: number;
  preset: string;
  status: string;
  width: number;
  height: number;
  video: VideoDocument | null;
  videoTime: number;
  exportFormat: "png" | "jpg" | "svg";
  outputPath: string;
  crf: number;
  hold: number;
  exporting: boolean;
  exportProgress: [number, number] | null;
};

const emptyPipeline: Pipeline = {
  layers: [],
  animate_noise: false,
  composite: { enabled: false, mode: "Normal", opacity: 1, key: "None", key_threshold: 0.5, key_softness: 0.1 },
};
const layerNames: Record<string, string> = {
  Adjust: "Adjust",
  Blur: "Gaussian blur",
  Downscale: "Downscale",
  Upscale: "Upscale",
  Dither: "Dither",
  ChromaticAberration: "Chromatic aberration",
  Glow: "Glow",
  Scanlines: "Scanlines",
  Invert: "Invert",
  Noise: "Film grain",
  RowShift: "Row shift",
  JpegGlitch: "JPEG glitch",
  Posterize: "Posterize",
  GradientMap: "Gradient map",
  Sharpen: "Sharpen",
};

function newDocument(id: number): DocumentTab {
  const pipeline = structuredClone(emptyPipeline);
  return {
    id,
    name: "Untitled",
    source: null,
    previewSource: null,
    previewId: null,
    preview: null,
    pipeline,
    history: [structuredClone(pipeline)],
    historyIndex: 0,
    preset: "Custom",
    status: "Open an image to begin",
    width: 0,
    height: 0,
    video: null,
    videoTime: 0,
    exportFormat: "png",
    outputPath: "",
    crf: 14,
    hold: 0,
    exporting: false,
    exportProgress: null,
  };
}

function dataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(new Error("Could not read this file"));
    reader.readAsDataURL(file);
  });
}

function imageSize(src: string): Promise<[number, number]> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve([image.naturalWidth, image.naturalHeight]);
    image.onerror = () => reject(new Error("This image could not be decoded"));
    image.src = src;
  });
}

function niceName(type: string) {
  return layerNames[type] ?? type.replace(/([A-Z])/g, " $1").trim();
}

function defaultLayer(type: string): Layer {
  const defaults: Record<string, Layer> = {
    Adjust: { type, enabled: true, opacity: 1, brightness: 0, contrast: 1, gamma: 1, saturation: 1 },
    Blur: { type, enabled: true, opacity: 1, radius: 2 },
    Downscale: { type, enabled: true, opacity: 1, factor: 2 },
    Upscale: { type, enabled: true, opacity: 1, factor: 2 },
    Dither: {
      type,
      enabled: true,
      opacity: 1,
      method: { algo: "Diffusion", kernel: "FloydSteinberg", serpentine: true, strength: 1 },
      palette: { kind: "Mono" },
    },
    ChromaticAberration: { type, enabled: true, opacity: 1, shift: 3 },
    Glow: { type, enabled: true, opacity: 1, threshold: 0.6, radius: 6, intensity: 0.5 },
    Noise: { type, enabled: true, opacity: 1, amount: 0.1, seed: 1 },
    RowShift: { type, enabled: true, opacity: 1, max_shift: 12, chance: 0.05, seed: 1 },
    JpegGlitch: { type, enabled: true, opacity: 1, quality: 25, corrupt: 0.05, seed: 1 },
    Invert: { type, enabled: true, opacity: 1 },
    Posterize: { type, enabled: true, opacity: 1, levels: 4 },
    Scanlines: { type, enabled: true, opacity: 1, period: 3, strength: 0.25 },
    Sharpen: { type, enabled: true, opacity: 1, amount: 0.6, radius: 1 },
    GradientMap: { type, enabled: true, opacity: 1, colors: ["#14123a", "#c2386b", "#fbe9c6"] },
  };
  return defaults[type] ?? { type, enabled: true, opacity: 1 };
}

function defaultMethod(algo: string): Record<string, unknown> {
  const variants: Record<string, Record<string, unknown>> = {
    Diffusion: { algo, kernel: "FloydSteinberg", serpentine: true, strength: 1 },
    Bayer: { algo, size: 4, spread: null },
    Halftone: { algo, cell: 6, angle: 45, spread: null },
    Lines: { algo, period: 5, angle: 20, spread: null },
    WhiteNoise: { algo, seed: 1, spread: null },
    Ign: { algo, spread: null },
    BlueNoise: { algo, seed: 1, spread: null },
  };
  return variants[algo] ?? variants.Diffusion;
}

function defaultPalette(kind: string): Record<string, unknown> {
  const variants: Record<string, Record<string, unknown>> = {
    Mono: { kind }, Gray: { kind, levels: 4 }, Duo: { kind, dark: "#1b1b3a", light: "#f2e9c9" },
    Gameboy: { kind }, Cga: { kind }, Custom: { kind, colors: ["#000000", "#ff5555", "#ffffff"] }, Extract: { kind, count: 8 },
  };
  return variants[kind] ?? variants.Mono;
}

function numericRange(key: string): [number, number, number] {
  const ranges: Record<string, [number, number, number]> = {
    brightness: [-0.5, 0.5, 0.01], contrast: [0.2, 3, 0.01], gamma: [0.2, 3, 0.01],
    saturation: [0, 2, 0.01], opacity: [0, 1, 0.01], strength: [0, 1.2, 0.01],
    intensity: [0, 2, 0.01], threshold: [0, 1, 0.01], key_threshold: [0, 1, 0.01], key_softness: [0, 0.5, 0.01], angle: [-90, 90, 1],
    amount: [0, 3, 0.01], chance: [0, 1, 0.01], corrupt: [0, 1, 0.01],
    radius: [0, 40, 1], period: [2, 24, 1], levels: [2, 32, 1], shift: [-30, 30, 1],
    factor: [1, 16, 1], cell: [3, 24, 0.5], size: [2, 16, 1], quality: [1, 100, 1],
    max_shift: [0, 100, 1], seed: [0, 9999, 1],
  };
  return ranges[key] ?? [0, 100, 1];
}

function formatValue(value: number) {
  return Number.isInteger(value) ? String(value) : value.toFixed(2).replace(/0+$/, "").replace(/\.$/, "");
}

export default function App() {
  const [documents, setDocuments] = useState<DocumentTab[]>([newDocument(1)]);
  const [activeId, setActiveId] = useState(1);
  const [presets, setPresets] = useState<string[]>([]);
  const [selectedLayer, setSelectedLayer] = useState(0);
  const [zoom, setZoom] = useState(100);
  const [showOriginal, setShowOriginal] = useState(false);
  const [splitView, setSplitView] = useState(false);
  const [split, setSplit] = useState(50);
  const [busy, setBusy] = useState(false);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const fileInput = useRef<HTMLInputElement>(null);
  const renderSequence = useRef(0);
  const renderQueue = useRef<Promise<void>>(Promise.resolve());
  const historyEdit = useRef<{ id: number; time: number } | null>(null);
  const dragOrigin = useRef<{ x: number; y: number; panX: number; panY: number } | null>(null);
  const active = documents.find((document) => document.id === activeId) ?? documents[0];
  const layer = active.pipeline.layers[selectedLayer];

  useEffect(() => {
    let cancelled = false;
    invoke<string[]>("list_presets")
      .then(async (names) => {
        if (cancelled) return;
        setPresets(names);
        const first = names[0];
        if (first) {
          const json = await invoke<string>("get_preset", { name: first });
          if (!cancelled) {
            const pipeline = JSON.parse(json) as Pipeline;
            setDocuments((items) => items.map((item) => item.id === 1 ? { ...item, pipeline, history: [structuredClone(pipeline)], historyIndex: 0, preset: first } : item));
          }
        }
      })
      .catch(() => setDocuments((items) => items.map((item) => ({ ...item, status: "Tauri backend unavailable" }))));
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    void listen<{ done: number; total: number }>("export-progress", ({ payload }) => {
      setDocuments((items) => items.map((item) => item.exporting ? { ...item, exportProgress: [payload.done, payload.total] } : item));
    }).then((stop) => { unlisten = stop; });
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    const sequence = ++renderSequence.current;
    if (active.previewId === null) {
      setBusy(false);
      return;
    }
    let cancelled = false;
    const timer = window.setTimeout(async () => {
      renderQueue.current = renderQueue.current.catch(() => {}).then(async () => {
        if (cancelled || sequence !== renderSequence.current) return;
        setBusy(true);
        try {
          const pipelineJson = JSON.stringify(active.pipeline);
          const png = await invoke<string>("render_preview", { previewId: active.previewId, pipelineJson });
          if (!cancelled && sequence === renderSequence.current) {
            setDocuments((items) => items.map((item) => item.id === active.id ? { ...item, preview: `data:image/png;base64,${png}`, status: "Preview ready" } : item));
          }
        } catch (error) {
          if (!cancelled && sequence === renderSequence.current) setDocuments((items) => items.map((item) => item.id === active.id ? { ...item, status: String(error) } : item));
        } finally {
          if (!cancelled && sequence === renderSequence.current) setBusy(false);
        }
      });
    }, 60);
    return () => { cancelled = true; window.clearTimeout(timer); };
  }, [active.id, active.pipeline, active.previewId]);

  useEffect(() => {
    if (!active.video) return;
    let cancelled = false;
    const timer = window.setTimeout(() => {
      void invoke<string>("video_frame", { path: active.video!.path, seconds: active.videoTime })
        .then(async (png) => {
          if (cancelled) return;
          const source = `data:image/png;base64,${png}`;
          const [width, height] = await imageSize(source);
          const previewSource = await makePreview(source, width, height);
          const previewId = await invoke<number>("prepare_preview", { imageData: previewSource });
          if (cancelled) {
            void invoke("release_preview", { previewId });
            return;
          }
          releasePreview(active.previewId, active.id);
          patchDocument(active.id, { source, previewSource, previewId, preview: source, width, height, status: `Frame at ${active.videoTime.toFixed(2)} s` });
        })
        .catch((error) => { if (!cancelled) patchDocument(active.id, { status: String(error) }); });
    }, 160);
    return () => { cancelled = true; window.clearTimeout(timer); };
  }, [active.id, active.video, active.videoTime]);

  const enabledCount = useMemo(() => active.pipeline.layers.filter((item) => item.enabled).length, [active.pipeline.layers]);

  function patchDocument(id: number, patch: Partial<DocumentTab>) {
    setDocuments((items) => items.map((item) => item.id === id ? { ...item, ...patch } : item));
  }

  function releasePreview(previewId: number | null, documentId: number) {
    if (previewId === null || documents.some((item) => item.id !== documentId && item.previewId === previewId)) return;
    void invoke("release_preview", { previewId }).catch(() => {});
  }

  async function openDocumentSource(name: string, source: string, video: VideoDocument | null = null) {
    const [width, height] = await imageSize(source);
    const previewSource = await makePreview(source, width, height);
    const previewId = await invoke<number>("prepare_preview", { imageData: previewSource });
    const isBlank = !active.source && active.name === "Untitled";
    let targetId = active.id;
    if (!isBlank) {
      targetId = Math.max(...documents.map((item) => item.id)) + 1;
      const pipeline = structuredClone(active.pipeline);
      setDocuments((items) => [...items, { ...newDocument(targetId), pipeline, history: [structuredClone(pipeline)], preset: active.preset }]);
      setActiveId(targetId);
    }
    setSelectedLayer(0);
    patchDocument(targetId, {
      name,
      source,
      previewSource,
      previewId,
      preview: source,
      width,
      height,
      video,
      videoTime: 0,
      status: video ? `Video ${video.width} × ${video.height} at ${video.fps.toFixed(2)} fps` : `${width} × ${height}`,
    });
    return targetId;
  }

  async function openFile(file?: File) {
    if (!file) return;
    if (!/^image\/(png|jpeg)$/.test(file.type)) {
      patchDocument(active.id, { status: "Choose a PNG or JPEG image" });
      return;
    }
    try {
      await openDocumentSource(file.name, await dataUrl(file));
    } catch (error) {
      patchDocument(active.id, { status: String(error) });
    }
  }

  async function openNativePath(path: string) {
    try {
      const name = path.split(/[\\/]/).pop() ?? path;
      if (/\.(mp4|mov|mkv|webm|avi|m4v|gif)$/i.test(path)) {
        const loaded = JSON.parse(await invoke<string>("load_video", { path })) as { width: number; height: number; fps: number; duration: number; frame: string };
        await openDocumentSource(name, `data:image/png;base64,${loaded.frame}`, { path, width: loaded.width, height: loaded.height, fps: loaded.fps, duration: loaded.duration });
      } else {
        await openDocumentSource(name, await invoke<string>("read_image", { path }));
      }
    } catch (error) {
      patchDocument(active.id, { status: String(error) });
    }
  }

  async function openNativeMedia() {
    try {
      const path = await invoke<string | null>("pick_media");
      if (path) await openNativePath(path);
    } catch (error) {
      patchDocument(active.id, { status: String(error) });
    }
  }

  useEffect(() => {
    if (!isTauri()) return;
    let mounted = true;
    let unlisten: (() => void) | undefined;
    void listen<string>("media-dropped", ({ payload }) => {
      void openNativePath(payload);
    }).then((stop) => {
      if (mounted) unlisten = stop;
      else stop();
    }).catch(() => {});
    return () => { mounted = false; unlisten?.(); };
  }, []);

  function onFileChange(event: ChangeEvent<HTMLInputElement>) {
    void openFile(event.target.files?.[0]);
    event.target.value = "";
  }

  function onDrop(event: DragEvent<HTMLElement>) {
    event.preventDefault();
    event.stopPropagation();
    void openFile(event.dataTransfer.files[0]);
  }

  async function choosePreset(name: string) {
    try {
      const json = await invoke<string>("get_preset", { name });
      const pipeline = JSON.parse(json) as Pipeline;
      applyPipeline(pipeline, name);
    } catch (error) {
      patchDocument(active.id, { status: String(error) });
    }
  }

  function updatePipeline(update: (pipeline: Pipeline) => void) {
    const now = Date.now();
    setDocuments((items) => items.map((item) => {
      if (item.id !== active.id) return item;
      const pipeline = structuredClone(item.pipeline);
      update(pipeline);
      const history = item.history.slice(0, item.historyIndex + 1);
      const previousEdit = historyEdit.current;
      const coalesce = previousEdit?.id === item.id && now - previousEdit.time < 300 && item.historyIndex === item.history.length - 1;
      if (coalesce && history.length > 1) history[history.length - 1] = structuredClone(pipeline);
      else history.push(structuredClone(pipeline));
      historyEdit.current = { id: item.id, time: now };
      return { ...item, pipeline, history, historyIndex: history.length - 1, preset: "Custom" };
    }));
  }

  function changeHistory(direction: -1 | 1) {
    const next = active.historyIndex + direction;
    if (next < 0 || next >= active.history.length) return;
    patchDocument(active.id, { pipeline: structuredClone(active.history[next]), historyIndex: next, preset: "Custom" });
    setSelectedLayer((index) => Math.max(0, Math.min(index, active.history[next].layers.length - 1)));
    historyEdit.current = null;
  }

  function applyPipeline(pipeline: Pipeline, preset: string) {
    const copy = structuredClone(pipeline);
    const history = active.history.slice(0, active.historyIndex + 1);
    history.push(structuredClone(copy));
    patchDocument(active.id, { pipeline: copy, history, historyIndex: history.length - 1, preset });
    setSelectedLayer(0);
    historyEdit.current = null;
  }

  function updateLayer(path: (string | number)[], value: unknown) {
    updatePipeline((pipeline) => {
      let target: any = pipeline.layers[selectedLayer];
      for (const part of path.slice(0, -1)) target = target[part];
      target[path[path.length - 1]] = value;
    });
  }

  function editColorArray(path: (string | number)[], index: number | null) {
    updatePipeline((pipeline) => {
      let target: any = pipeline.layers[selectedLayer];
      for (const part of path) target = target[part];
      if (!Array.isArray(target)) return;
      if (index === null) target.push("#808080");
      else if (target.length > 2) target.splice(index, 1);
    });
  }

  function addLayer(type: string) {
    updatePipeline((pipeline) => pipeline.layers.push(defaultLayer(type)));
    setSelectedLayer(active.pipeline.layers.length);
  }

  function removeLayer(index: number) {
    updatePipeline((pipeline) => { pipeline.layers.splice(index, 1); });
    setSelectedLayer(Math.max(0, Math.min(selectedLayer, active.pipeline.layers.length - 2)));
  }

  function moveLayer(index: number, direction: number) {
    updatePipeline((pipeline) => {
      const next = index + direction;
      if (next >= 0 && next < pipeline.layers.length) [pipeline.layers[index], pipeline.layers[next]] = [pipeline.layers[next], pipeline.layers[index]];
    });
    setSelectedLayer(index + direction);
  }

  function reorderLayer(from: number, to: number) {
    if (from === to || from < 0 || to < 0 || from >= active.pipeline.layers.length || to >= active.pipeline.layers.length) return;
    updatePipeline((pipeline) => {
      const [moved] = pipeline.layers.splice(from, 1);
      pipeline.layers.splice(to, 0, moved);
    });
    setSelectedLayer(to);
  }

  async function exportImage() {
    if (!active.source) return;
    setBusy(true);
    try {
      const pipelineJson = JSON.stringify(active.pipeline);
      const format = active.video ? "mp4" : active.exportFormat;
      const outputPath = active.outputPath || await invoke<string | null>("pick_output_path", { format });
      if (!outputPath) return;
      patchDocument(active.id, { outputPath });
      if (active.video) {
        patchDocument(active.id, { exporting: true, exportProgress: [0, 1], status: "Encoding video..." });
        const status = await invoke<string>("export_video", {
          path: active.video.path,
          outputPath,
          pipelineJson,
          crf: active.crf,
          hold: active.hold,
        });
        patchDocument(active.id, { status });
      } else {
        const status = await invoke<string>("export_image", { imageData: active.source, pipelineJson, outputPath });
        patchDocument(active.id, { status });
      }
    } catch (error) {
      patchDocument(active.id, { status: String(error) });
    } finally {
      setBusy(false);
      patchDocument(active.id, { exporting: false, exportProgress: null });
    }
  }

  async function processBatch() {
    patchDocument(active.id, { exporting: true, exportProgress: null, status: "Choose input and output folders..." });
    try {
      const status = await invoke<string>("process_batch", { pipelineJson: JSON.stringify(active.pipeline) });
      patchDocument(active.id, { status });
    } catch (error) {
      patchDocument(active.id, { status: String(error) });
    } finally {
      patchDocument(active.id, { exporting: false, exportProgress: null });
    }
  }

  async function cancelExport() {
    await invoke("cancel_export");
    patchDocument(active.id, { status: "Cancelling..." });
  }

  async function savePresetFile() {
    try {
      patchDocument(active.id, { status: await invoke<string>("save_preset_file", { pipelineJson: JSON.stringify(active.pipeline) }) });
    } catch (error) {
      patchDocument(active.id, { status: String(error) });
    }
  }

  async function loadPresetFile() {
    try {
      const json = await invoke<string>("load_preset_file");
      applyPipeline(JSON.parse(json) as Pipeline, "Custom");
      patchDocument(active.id, { status: "Preset loaded" });
    } catch (error) {
      patchDocument(active.id, { status: String(error) });
    }
  }

  function closeTab(id: number) {
    const closing = documents.find((item) => item.id === id);
    if (closing) releasePreview(closing.previewId, id);
    if (documents.length === 1) {
      setDocuments([newDocument(id + 1)]);
      setActiveId(id + 1);
      return;
    }
    const remaining = documents.filter((item) => item.id !== id);
    setDocuments(remaining);
    if (id === activeId) setActiveId(remaining[Math.max(0, documents.findIndex((item) => item.id === id) - 1)].id);
  }

  function addTab() {
    const id = Math.max(...documents.map((item) => item.id)) + 1;
    const pipeline = structuredClone(active.pipeline);
    setDocuments((items) => [...items, { ...newDocument(id), pipeline, history: [structuredClone(pipeline)], preset: active.preset }]);
    setActiveId(id);
  }

  function duplicateTab() {
    const id = Math.max(...documents.map((item) => item.id)) + 1;
    const duplicate = { ...active, id, pipeline: structuredClone(active.pipeline), history: [structuredClone(active.pipeline)], historyIndex: 0, exporting: false, exportProgress: null };
    setDocuments((items) => [...items, duplicate]);
    setActiveId(id);
  }

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (!(event.ctrlKey || event.metaKey)) return;
      const target = event.target as HTMLElement;
      if (["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName) && !["z", "y"].includes(event.key.toLowerCase())) return;
      const key = event.key.toLowerCase();
      if (key === "z") {
        event.preventDefault();
        changeHistory(event.shiftKey ? 1 : -1);
      } else if (key === "y") {
        event.preventDefault();
        changeHistory(1);
      } else if (key === "o") {
        event.preventDefault();
        void openNativeMedia();
      } else if (key === "s") {
        event.preventDefault();
        void exportImage();
      } else if (key === "t") {
        event.preventDefault();
        addTab();
      } else if (key === "w") {
        event.preventDefault();
        closeTab(active.id);
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [active.id, active.historyIndex, active.history, active.source, active.pipeline]);

  function renderSetting(value: unknown, path: (string | number)[], key: string): React.ReactNode {
    if (key === "spread") {
      return <div className="spread-setting" key={path.join(".")}>
        <label className="check-setting"><input type="checkbox" checked={value !== null} onChange={(event) => updateLayer(path, event.target.checked ? 1 : null)} /><span>Custom spread</span></label>
        {typeof value === "number" && <label className="setting"><span className="setting-heading"><span>Spread</span><output>{formatValue(value)}</output></span><input className="range" type="range" min="0.05" max="2" step="0.01" value={value} onChange={(event) => updateLayer(path, Number(event.target.value))} /></label>}
      </div>;
    }
    if (typeof value === "number") {
      const [min, max, step] = numericRange(key);
      if (key === "size") return <label className="setting" key={path.join(".")}><span className="setting-heading">Matrix</span><select value={value} onChange={(event) => updateLayer(path, Number(event.target.value))}>{[2, 4, 8, 16].map((size) => <option value={size} key={size}>{size} × {size}</option>)}</select></label>;
      const isSeed = key === "seed";
      return (
        <label className="setting" key={path.join(".")}>
          <span className="setting-heading"><span>{key.replace(/_/g, " ")}</span><output>{formatValue(value)}</output></span>
          {isSeed ? <input type="number" min={min} max={max} step={step} value={value} onChange={(event) => updateLayer(path, Number(event.target.value))} /> :
            <input className="range" type="range" min={min} max={max} step={step} value={value} onChange={(event) => updateLayer(path, Number(event.target.value))} />}
        </label>
      );
    }
    if (typeof value === "boolean") {
      return <label className="check-setting" key={path.join(".")}><input type="checkbox" checked={value} onChange={(event) => updateLayer(path, event.target.checked)} /><span>{key.replace(/_/g, " ")}</span></label>;
    }
    if (typeof value === "string" && key !== "type") {
      if (key === "algo") {
        const methods = ["Diffusion", "Bayer", "Halftone", "Lines", "WhiteNoise", "Ign", "BlueNoise"];
        return <label className="setting" key={path.join(".")}><span className="setting-heading">Method</span><select value={value} onChange={(event) => updateLayer(path.slice(0, -1), defaultMethod(event.target.value))}>{methods.map((method) => <option value={method} key={method}>{method.replace(/([A-Z])/g, " $1").trim()}</option>)}</select></label>;
      }
      if (key === "kind") {
        const palettes = ["Mono", "Gray", "Duo", "Gameboy", "Cga", "Custom", "Extract"];
        return <label className="setting" key={path.join(".")}><span className="setting-heading">Palette</span><select value={value} onChange={(event) => updateLayer(path.slice(0, -1), defaultPalette(event.target.value))}>{palettes.map((kind) => <option value={kind} key={kind}>{kind}</option>)}</select></label>;
      }
      const options = key === "kernel"
        ? ["FloydSteinberg", "JarvisJudiceNinke", "Stucki", "Atkinson", "Burkes", "Sierra", "SierraTwoRow", "SierraLite", "ShiauFan", "ShiauFan2"]
        : [];
      if (options.length) return <label className="setting" key={path.join(".")}><span className="setting-heading">{key}</span><select value={value} onChange={(event) => updateLayer(path, event.target.value)}>{options.map((option) => <option key={option}>{option}</option>)}</select></label>;
      if (value.startsWith("#")) return <label className="setting color-setting" key={path.join(".")}><span>{key.replace(/_/g, " ")}</span><input type="color" value={/^#[0-9a-f]{6}$/i.test(value) ? value : "#808080"} onChange={(event) => updateLayer(path, event.target.value)} /><input value={value} onChange={(event) => updateLayer(path, event.target.value)} /></label>;
    }
    if (Array.isArray(value)) {
      return <div className="array-setting" key={path.join(".")}><div className="setting-heading">{key.replace(/_/g, " ")}</div>{value.map((item, index) => <div className="color-stop" key={`${path.join(".")}.${index}`}>{renderSetting(item, [...path, index], `${key} ${index + 1}`)}<button className="icon-button small" aria-label={`Remove color ${index + 1}`} title="Remove color" disabled={value.length <= 2} onClick={() => editColorArray(path, index)}><X size={12} /></button></div>)}<button className="add-color" onClick={() => editColorArray(path, null)}>Add color stop</button></div>;
    }
    if (value && typeof value === "object") {
      return <div className="nested-settings" key={path.join(".")}>{Object.entries(value as Record<string, unknown>).map(([childKey, child]) => renderSetting(child, [...path, childKey], childKey))}</div>;
    }
    return null;
  }

  return (
    <main className="editor" onDrop={onDrop} onDragOver={(event) => event.preventDefault()}>
      <header className="topbar">
        <div className="titlebar-drag-area" data-tauri-drag-region />
        <div className="brand" data-tauri-drag-region><img className="brand-logo" src={brandLogo} alt="" /><span data-tauri-drag-region>dotwise</span></div>
        <div className="top-separator" />
        <button className="top-action" onClick={() => void openNativeMedia()}><FolderOpen size={15} />Open media</button>
        <button className="icon-button" title="Undo" aria-label="Undo" disabled={active.historyIndex === 0} onClick={() => changeHistory(-1)}><Undo2 size={15} /></button>
        <button className="icon-button" title="Redo" aria-label="Redo" disabled={active.historyIndex >= active.history.length - 1} onClick={() => changeHistory(1)}><Redo2 size={15} /></button>
        <button className="top-action" disabled={active.exporting} onClick={() => void processBatch()}><FolderSync size={15} />Batch folder</button>
        {!active.video && <select className="export-format" aria-label="Export format" value={active.exportFormat} onChange={(event) => patchDocument(active.id, { exportFormat: event.target.value as DocumentTab["exportFormat"], outputPath: "" })}><option value="png">PNG</option><option value="jpg">JPEG</option><option value="svg">SVG</option></select>}
        <button className="top-action" disabled={!active.source || active.exporting || busy} onClick={() => void exportImage()}><Save size={15} />{active.video ? "Export video" : "Export"}</button>
        {active.exporting && <button className="icon-button" title="Cancel export" aria-label="Cancel export" onClick={() => void cancelExport()}><X size={15} /></button>}
        <div className="top-spacer" data-tauri-drag-region />
        <span className="engine-indicator"><span className={`status-dot ${busy || active.exporting ? "working" : ""}`} />{active.exporting ? "Exporting" : busy ? "Rendering" : "Ready"}</span>
        {isTauri() && <div className="window-controls">
          <button aria-label="Minimize window" title="Minimize" onClick={() => void getCurrentWindow().minimize()}><Minus size={15} /></button>
          <button aria-label="Maximize window" title="Maximize" onClick={() => void getCurrentWindow().toggleMaximize()}><Maximize2 size={14} /></button>
          <button className="window-close" aria-label="Close window" title="Close" onClick={() => void getCurrentWindow().close()}><X size={15} /></button>
        </div>}
        <input ref={fileInput} className="file-input" type="file" accept="image/png,image/jpeg" onChange={onFileChange} />
      </header>

      <div className="document-bar">
        <div className="tabs" role="tablist">
          {documents.map((document) => (
            <div className={`document-tab ${active.id === document.id ? "active" : ""}`} key={document.id} role="tab" aria-selected={active.id === document.id} onClick={() => setActiveId(document.id)}>
              <FileImage size={14} /><span>{document.name}</span><button aria-label={`Close ${document.name}`} onClick={(event) => { event.stopPropagation(); closeTab(document.id); }}><X size={13} /></button>
            </div>
          ))}
          <button className="icon-button add-tab" aria-label="New tab" title="New tab" onClick={addTab}><Plus size={15} /></button>
          <button className="icon-button add-tab" aria-label="Duplicate tab" title="Duplicate tab" onClick={duplicateTab}><Copy size={14} /></button>
        </div>
        <div className="document-tools">
          <label className="preset-picker"><span>Preset</span><select value={active.preset} onChange={(event) => void choosePreset(event.target.value)}><option value="Custom">Custom</option>{presets.map((preset) => <option key={preset}>{preset}</option>)}</select><ChevronDown size={13} /></label>
          <div className="tool-divider" />
          <button className="icon-button" title="Zoom out" aria-label="Zoom out" onClick={() => setZoom((value) => Math.max(10, value - 10))}><ZoomOut size={15} /></button>
          <button className="zoom-value" onClick={() => setZoom(100)}>{zoom}%</button>
          <button className="icon-button" title="Zoom in" aria-label="Zoom in" onClick={() => setZoom((value) => Math.min(400, value + 10))}><ZoomIn size={15} /></button>
          <div className="tool-divider" />
          <button className={`icon-button ${showOriginal ? "selected" : ""}`} title={showOriginal ? "Show result" : "Show original"} aria-label={showOriginal ? "Show result" : "Show original"} onClick={() => setShowOriginal((value) => !value)}>{showOriginal ? <EyeOff size={15} /> : <Eye size={15} />}</button>
          <button className={`icon-button ${splitView ? "selected" : ""}`} title="Split comparison" aria-label="Split comparison" onClick={() => setSplitView((value) => !value)}><Columns2 size={15} /></button>
          {splitView && <input className="split-range" aria-label="Comparison position" type="range" min="0" max="100" value={split} onChange={(event) => setSplit(Number(event.target.value))} />}
          {active.video && <label className="video-time"><Video size={13} /><input type="range" min="0" max={Math.max(0.1, active.video.duration)} step="0.01" value={active.videoTime} onChange={(event) => patchDocument(active.id, { videoTime: Number(event.target.value) })} /><span>{active.videoTime.toFixed(1)}s</span></label>}
        </div>
      </div>

      <section className="workspace">
        <nav className="tool-rail" aria-label="Canvas tools">
          <button className="rail-button selected" title="Move canvas"><span className="move-icon">✥</span></button>
          <button className="rail-button" title="Image information" onClick={() => patchDocument(active.id, { status: `${active.width} × ${active.height} px` })}><ImagePlus size={17} /></button>
          <div className="rail-rule" />
          <button className="rail-button" title="Fit image" onClick={() => { setZoom(100); setPan({ x: 0, y: 0 }); }}><RefreshCw size={16} /></button>
          <div className="rail-bottom"><Layers3 size={16} /></div>
        </nav>

        <section
          className="canvas-area"
          onDrop={onDrop}
          onDragOver={(event) => event.preventDefault()}
          onWheel={(event) => setZoom((value) => Math.max(10, Math.min(400, value * (event.deltaY < 0 ? 1.1 : 0.9))))}
          onPointerDown={(event) => {
            if (!active.source || event.button !== 0) return;
            dragOrigin.current = { x: event.clientX, y: event.clientY, panX: pan.x, panY: pan.y };
            event.currentTarget.setPointerCapture(event.pointerId);
          }}
          onPointerMove={(event) => {
            const origin = dragOrigin.current;
            if (origin) setPan({ x: origin.panX + event.clientX - origin.x, y: origin.panY + event.clientY - origin.y });
          }}
          onPointerUp={() => { dragOrigin.current = null; }}
        >
          {active.source ? (
            <div className="canvas-frame" style={{ transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom / 100})` }}>
              <img src={showOriginal ? active.source : active.preview ?? active.source} alt={active.name} draggable={false} />
              {splitView && !showOriginal && <>
                <img className="canvas-before" src={active.source} alt="Original" draggable={false} style={{ clipPath: `inset(0 ${100 - split}% 0 0)` }} />
                <span className="split-divider" style={{ left: `${split}%` }} />
              </>}
            </div>
          ) : (
            <button className="empty-state" onClick={() => void openNativeMedia()}>
              <span className="empty-icon"><ImagePlus size={24} /></span>
              <strong>Open an image or video</strong>
              <span>Drop a PNG or JPEG here</span>
              <span className="open-button"><FolderOpen size={14} />Browse files</span>
            </button>
          )}
          <div className="canvas-meta"><span>{active.width ? `${active.width} × ${active.height} px` : "No document"}</span><span className="meta-dot" />{active.source ? `${enabledCount} effects` : "RGB · 8-bit"}</div>
        </section>

        <aside className="inspector">
          <div className="panel-heading"><div><SlidersHorizontal size={15} /><span>Adjustments</span></div><button className="icon-button small" title="Add effect" aria-label="Add effect" onClick={() => addLayer("Adjust")}><Plus size={15} /></button></div>
          <div className="layer-summary"><span>Effect stack</span><span>{active.pipeline.layers.length} layers</span></div>
          <div className="layer-list">
            {active.pipeline.layers.map((item, index) => (
              <div className={`layer-row ${selectedLayer === index ? "selected" : ""}`} key={`${item.type}-${index}`} draggable onDragStart={(event) => event.dataTransfer.setData("application/x-dotwise-layer", String(index))} onDragOver={(event) => event.preventDefault()} onDrop={(event) => { event.preventDefault(); reorderLayer(Number(event.dataTransfer.getData("application/x-dotwise-layer")), index); }} onClick={() => setSelectedLayer(index)}>
                <button className="layer-eye" title={item.enabled ? "Disable effect" : "Enable effect"} aria-label={item.enabled ? "Disable effect" : "Enable effect"} onClick={(event) => { event.stopPropagation(); updatePipeline((pipeline) => { pipeline.layers[index].enabled = !pipeline.layers[index].enabled; }); }}>
                  {item.enabled ? <Eye size={14} /> : <EyeOff size={14} />}
                </button>
                <span className="layer-index">{String(index + 1).padStart(2, "0")}</span>
                <span className="layer-name">{niceName(item.type)}</span>
                <button className="layer-remove" aria-label={`Remove ${niceName(item.type)}`} title="Remove effect" onClick={(event) => { event.stopPropagation(); removeLayer(index); }}><X size={13} /></button>
              </div>
            ))}
            {active.pipeline.layers.length === 0 && <div className="no-layers">No effects in this stack</div>}
          </div>
          <label className="add-effect"><Plus size={14} /><span>Add effect</span><select aria-label="Add effect" value="" onChange={(event) => { if (event.target.value) addLayer(event.target.value); event.target.value = ""; }}><option value="" disabled>Choose effect</option>{Object.entries(layerNames).map(([type, label]) => <option key={type} value={type}>{label}</option>)}</select><ChevronDown size={13} /></label>

          <details className="composite-settings">
            <summary>Overlay on original</summary>
            <label className="check-setting"><input type="checkbox" checked={Boolean(active.pipeline.composite?.enabled)} onChange={(event) => updatePipeline((pipeline) => { pipeline.composite ??= { enabled: false, mode: "Normal", opacity: 1, key: "None", key_threshold: 0.5, key_softness: 0.1 }; pipeline.composite.enabled = event.target.checked; })} /><span>Lay result over original</span></label>
            {active.pipeline.composite?.enabled && <>
              <label className="setting"><span className="setting-heading">Blend mode</span><select value={active.pipeline.composite?.mode ?? "Normal"} onChange={(event) => updatePipeline((pipeline) => { pipeline.composite!.mode = event.target.value; })}>{["Normal", "Multiply", "Screen", "Overlay", "Add"].map((mode) => <option key={mode}>{mode}</option>)}</select></label>
              <label className="setting"><span className="setting-heading"><span>Opacity</span><output>{Math.round((active.pipeline.composite?.opacity ?? 1) * 100)}%</output></span><input className="range" type="range" min="0" max="1" step="0.01" value={active.pipeline.composite?.opacity ?? 1} onChange={(event) => updatePipeline((pipeline) => { pipeline.composite!.opacity = Number(event.target.value); })} /></label>
              <label className="setting"><span className="setting-heading">Key out</span><select value={active.pipeline.composite?.key ?? "None"} onChange={(event) => updatePipeline((pipeline) => { pipeline.composite!.key = event.target.value; })}>{["None", "DropLight", "DropDark"].map((mode) => <option value={mode} key={mode}>{mode === "None" ? "Nothing" : mode === "DropLight" ? "Light pixels" : "Dark pixels"}</option>)}</select></label>
              {active.pipeline.composite?.key !== "None" && <>
                <label className="setting"><span className="setting-heading"><span>Key threshold</span><output>{formatValue(active.pipeline.composite?.key_threshold ?? 0.5)}</output></span><input className="range" type="range" min="0" max="1" step="0.01" value={active.pipeline.composite?.key_threshold ?? 0.5} onChange={(event) => updatePipeline((pipeline) => { pipeline.composite!.key_threshold = Number(event.target.value); })} /></label>
                <label className="setting"><span className="setting-heading"><span>Key softness</span><output>{formatValue(active.pipeline.composite?.key_softness ?? 0.1)}</output></span><input className="range" type="range" min="0" max="0.5" step="0.01" value={active.pipeline.composite?.key_softness ?? 0.1} onChange={(event) => updatePipeline((pipeline) => { pipeline.composite!.key_softness = Number(event.target.value); })} /></label>
              </>}
            </>}
          </details>
          {active.video && <div className="video-controls">
            <div className="panel-heading"><div><Video size={14} /><span>Video export</span></div></div>
            <label className="setting"><span className="setting-heading"><span>CRF quality</span><output>{active.crf}</output></span><input className="range" type="range" min="0" max="40" step="1" value={active.crf} onChange={(event) => patchDocument(active.id, { crf: Number(event.target.value) })} /></label>
            <label className="setting"><span className="setting-heading"><span>Temporal hold</span><output>{formatValue(active.hold)}</output></span><input className="range" type="range" min="0" max="0.2" step="0.01" value={active.hold} onChange={(event) => patchDocument(active.id, { hold: Number(event.target.value) })} /></label>
            <label className="check-setting"><input type="checkbox" checked={Boolean(active.pipeline.animate_noise)} onChange={(event) => updatePipeline((pipeline) => { pipeline.animate_noise = event.target.checked; })} /><span>Animate noise per frame</span></label>
          </div>}

          <div className="inspector-divider" />
          <div className="panel-heading settings-title"><div><SlidersHorizontal size={15} /><span>Settings</span></div>{layer && <span className="selected-type">{niceName(layer.type)}</span>}</div>
          <div className="settings-scroll">
            {layer ? (
              <>
                <div className="layer-actions">
                  <button title="Move effect up" aria-label="Move effect up" disabled={selectedLayer === 0} onClick={() => moveLayer(selectedLayer, -1)}><ChevronLeft size={14} /></button>
                  <span>Layer {String(selectedLayer + 1).padStart(2, "0")}</span>
                  <button title="Move effect down" aria-label="Move effect down" disabled={selectedLayer === active.pipeline.layers.length - 1} onClick={() => moveLayer(selectedLayer, 1)}><ChevronRight size={14} /></button>
                </div>
                <label className="setting">
                  <span className="setting-heading"><span>Opacity</span><output>{Math.round((layer.opacity ?? 1) * 100)}%</output></span>
                  <input className="range" type="range" min="0" max="1" step="0.01" value={layer.opacity ?? 1} onChange={(event) => updateLayer(["opacity"], Number(event.target.value))} />
                </label>
                {Object.entries(layer).filter(([key]) => !["type", "enabled", "opacity"].includes(key)).map(([key, value]) => renderSetting(value, [key], key))}
                {layer.type === "GradientMap" && <p className="hint">Enter colors as hexadecimal values in the layer settings.</p>}
              </>
            ) : <p className="hint">Select a preset or add an effect to see its settings.</p>}
          </div>
          <div className="preset-footer"><span>Preset</span><span className="preset-name">{active.preset}</span><button onClick={() => void loadPresetFile()}>Load</button><button onClick={() => void savePresetFile()}>Save</button></div>
        </aside>
      </section>

      <footer className="statusbar"><span className={`status-dot ${busy || active.exporting ? "working" : ""}`} /><span className="status-message">{active.status}</span>{active.exportProgress && <progress className="export-progress" value={active.exportProgress[0]} max={Math.max(1, active.exportProgress[1])} />}<span className="status-right">{active.source ? `${active.width} × ${active.height} · RGB` : "Dotwise editor"}</span></footer>
    </main>
  );
}

function makePreview(src: string, width: number, height: number): Promise<string> {
  const longestSide = Math.max(width, height);
  if (longestSide <= 1536) return Promise.resolve(src);
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => {
      const scale = 1536 / longestSide;
      const canvas = document.createElement("canvas");
      canvas.width = Math.max(1, Math.round(width * scale));
      canvas.height = Math.max(1, Math.round(height * scale));
      const context = canvas.getContext("2d");
      if (!context) return reject(new Error("Could not prepare preview"));
      context.drawImage(image, 0, 0, canvas.width, canvas.height);
      resolve(canvas.toDataURL("image/png"));
    };
    image.onerror = () => reject(new Error("This image could not be decoded"));
    image.src = src;
  });
}