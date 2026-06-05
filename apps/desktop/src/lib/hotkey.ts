// Tiny hotkey utility — turns a KeyboardEvent into a canonical string like
// "Ctrl+Shift+K" and tests an event against a stored shortcut spec.

const MOD_ORDER: Array<"Ctrl" | "Alt" | "Shift" | "Meta"> = [
  "Ctrl",
  "Alt",
  "Shift",
  "Meta",
];

/** Turn a KeyboardEvent into something like "Ctrl+K" or "Ctrl+Shift+F". */
export function eventToHotkey(e: KeyboardEvent): string {
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  if (e.metaKey) parts.push("Meta");
  const key = normaliseKey(e.key);
  if (!key) return "";
  // Don't record bare modifier presses
  if (["Ctrl", "Alt", "Shift", "Meta"].includes(key)) return "";
  parts.push(key);
  return parts.join("+");
}

function normaliseKey(raw: string): string {
  if (raw === " ") return "Space";
  if (raw.length === 1) return raw.toUpperCase();
  // Make modifier names match the prefix vocabulary
  if (raw === "Control") return "Ctrl";
  return raw; // ArrowUp, Enter, Escape, F1...
}

/** True if this event matches the stored hotkey spec. */
export function matchesHotkey(e: KeyboardEvent, spec: string): boolean {
  if (!spec || !spec.trim()) return false;
  const wanted = parseHotkey(spec);
  if (!wanted.key) return false;
  if (e.ctrlKey !== wanted.ctrl) return false;
  if (e.altKey !== wanted.alt) return false;
  if (e.shiftKey !== wanted.shift) return false;
  if (e.metaKey !== wanted.meta) return false;
  const k = normaliseKey(e.key);
  return k.toUpperCase() === wanted.key.toUpperCase();
}

function parseHotkey(spec: string): {
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  meta: boolean;
  key: string;
} {
  const parts = spec
    .split("+")
    .map((p) => p.trim())
    .filter(Boolean);
  let ctrl = false, alt = false, shift = false, meta = false;
  let key = "";
  for (const p of parts) {
    const lower = p.toLowerCase();
    if (lower === "ctrl" || lower === "control") ctrl = true;
    else if (lower === "alt" || lower === "option") alt = true;
    else if (lower === "shift") shift = true;
    else if (lower === "meta" || lower === "cmd" || lower === "command" || lower === "win") meta = true;
    else key = p;
  }
  return { ctrl, alt, shift, meta, key };
}

/** Pretty display string. Reorders modifiers in canonical order. */
export function formatHotkey(spec: string): string {
  if (!spec) return "";
  const w = parseHotkey(spec);
  const parts: string[] = [];
  if (w.ctrl) parts.push("Ctrl");
  if (w.alt) parts.push("Alt");
  if (w.shift) parts.push("Shift");
  if (w.meta) parts.push("Meta");
  if (w.key) parts.push(w.key.toUpperCase());
  void MOD_ORDER;
  return parts.join("+");
}
