import { describe, expect, it } from "vitest";
import {
  ACTIONS,
  DEFAULT_BINDINGS,
  appActionFor,
  bindingsByAction,
  buildKeymap,
  parseBinding,
  type Keymap,
} from "./keybindings";

/**
 * A key event as the window delivers one. `code` is what the app reads, so the
 * tests give it; `key` is only the fallback for synthetic input with no scan
 * code (SendKeys, remote desktop, remappers), which has its own test.
 */
function press(code: string, modifiers: Partial<KeyboardEvent> = {}): KeyboardEvent {
  return {
    code,
    key: "",
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    metaKey: false,
    ...modifiers,
  } as KeyboardEvent;
}

const shipped: Keymap = buildKeymap().keymap;

describe("the shipped bindings", () => {
  it("name only real actions, and every binding parses", () => {
    for (const [action, binding] of Object.entries(DEFAULT_BINDINGS)) {
      expect(action in ACTIONS, `${action} is bound but is not an action`).toBe(true);
      expect(parseBinding(binding), binding).toHaveProperty("binding");
    }
  });

  it("leave unbound only what the PRD gives no key", () => {
    // Everything else in the palette is reachable from the keyboard out of the box.
    const unbound = Object.keys(ACTIONS).filter((action) => !(action in DEFAULT_BINDINGS));
    expect(unbound).toEqual(["open-diff", "view-terminal", "view-cards", "view-office", "open-setup"]);
  });

  it("bind no two actions to the same keys", () => {
    // A collision here would mean one action is unreachable out of the box, and
    // which one survives would depend on object key order.
    expect(Object.keys(DEFAULT_BINDINGS).length).toBe(shipped.size);
  });

  it("are the table in PRD §13", () => {
    expect(appActionFor(press("KeyN", { ctrlKey: true, shiftKey: true }), shipped)).toEqual({
      kind: "new-workspace",
    });
    expect(appActionFor(press("KeyB", { ctrlKey: true, shiftKey: true }), shipped)).toEqual({
      kind: "toggle-sidebar",
    });
    expect(appActionFor(press("Digit3", { ctrlKey: true }), shipped)).toEqual({
      kind: "switch-workspace",
      index: 2,
    });
    expect(appActionFor(press("PageDown", { ctrlKey: true, shiftKey: true }), shipped)).toEqual({
      kind: "next-workspace",
    });
    expect(appActionFor(press("KeyD", { ctrlKey: true, shiftKey: true }), shipped)).toEqual({
      kind: "split-pane",
      direction: "right",
    });
    expect(appActionFor(press("KeyE", { ctrlKey: true, shiftKey: true }), shipped)).toEqual({
      kind: "split-pane",
      direction: "down",
    });
    expect(appActionFor(press("Space", { ctrlKey: true, shiftKey: true }), shipped)).toEqual({
      kind: "cycle-layout",
    });
    expect(appActionFor(press("ArrowLeft", { altKey: true }), shipped)).toEqual({
      kind: "focus-pane",
      direction: "left",
    });
    expect(appActionFor(press("ArrowLeft", { altKey: true, shiftKey: true }), shipped)).toEqual({
      kind: "move-pane",
      direction: "left",
    });
  });

  it("leave the keys a terminal needs alone", () => {
    // Ctrl+D is EOF, Ctrl+W deletes a word, Ctrl+P and Ctrl+N are history.
    for (const code of ["KeyD", "KeyW", "KeyP", "KeyN", "KeyF", "BracketLeft"]) {
      expect(appActionFor(press(code, { ctrlKey: true }), shipped), code).toBeNull();
    }
    expect(appActionFor(press("KeyA"), shipped)).toBeNull();
    expect(appActionFor(press("KeyC", { ctrlKey: true }), shipped)).toBeNull();
    expect(appActionFor(press("KeyV", { ctrlKey: true }), shipped)).toBeNull();
  });

  it("do not fire on a near miss", () => {
    // An extra modifier is a different combination, not the same one.
    expect(appActionFor(press("ArrowLeft", { altKey: true, ctrlKey: true }), shipped)).toBeNull();
    expect(appActionFor(press("KeyN", { ctrlKey: true, shiftKey: true, metaKey: true }), shipped)).toBeNull();
    expect(appActionFor(press("Digit3", { ctrlKey: true, shiftKey: true }), shipped)).toBeNull();
  });
});

describe("reading the key off an event", () => {
  it("uses the physical key, so the layout does not matter", () => {
    expect(appActionFor(press("KeyN", { ctrlKey: true, shiftKey: true }), shipped)).toEqual({
      kind: "new-workspace",
    });
  });

  it("falls back to `key` when there is no scan code", () => {
    // Synthetic input — SendKeys, remote desktop, remappers — often has none.
    const synthetic = {
      code: "",
      key: "n",
      ctrlKey: true,
      shiftKey: true,
      altKey: false,
      metaKey: false,
    } as KeyboardEvent;
    expect(appActionFor(synthetic, shipped)).toEqual({ kind: "new-workspace" });
  });

  it("treats the numpad Enter as Enter", () => {
    expect(appActionFor(press("NumpadEnter", { ctrlKey: true, shiftKey: true }), shipped)).toEqual({
      kind: "toggle-zoom",
    });
  });
});

describe("parsing a written binding", () => {
  it("accepts the spellings someone would actually write", () => {
    for (const written of ["Ctrl+Shift+Left", "control+shift+ArrowLeft", " ctrl + shift + left "]) {
      expect(parseBinding(written), written).toEqual({ binding: "Ctrl+Shift+ArrowLeft" });
    }
  });

  it("puts the modifiers in one order, so a binding has one spelling", () => {
    expect(parseBinding("Shift+Alt+Ctrl+K")).toEqual({ binding: "Ctrl+Alt+Shift+K" });
    expect(parseBinding("ctrl+alt+shift+k")).toEqual({ binding: "Ctrl+Alt+Shift+K" });
  });

  it("refuses a binding with no modifier, and says why", () => {
    // It would take an ordinary letter from the terminal on every keystroke,
    // which looks like a broken keyboard rather than like a setting.
    const parsed = parseBinding("A");
    expect(parsed).toHaveProperty("problem");
    expect("problem" in parsed && parsed.problem).toContain("no modifier");
  });

  it("refuses a binding with no key", () => {
    expect(parseBinding("Ctrl+Shift")).toHaveProperty("problem");
    expect(parseBinding("")).toHaveProperty("problem");
  });

  it("refuses a modifier it does not have", () => {
    const parsed = parseBinding("Hyper+K");
    expect(parsed).toHaveProperty("problem");
    expect("problem" in parsed && parsed.problem).toContain("Hyper");
  });
});

describe("applying the owner's overrides", () => {
  it("rebinds an action and gives up its old keys", () => {
    const { keymap, problems } = buildKeymap({ "new-workspace": "ctrl+alt+t" });
    expect(problems).toEqual([]);
    expect(appActionFor(press("KeyT", { ctrlKey: true, altKey: true }), keymap)).toEqual({
      kind: "new-workspace",
    });
    expect(appActionFor(press("KeyN", { ctrlKey: true, shiftKey: true }), keymap)).toBeNull();
  });

  it("lets an override take over another action's keys", () => {
    // Otherwise rebinding would fail silently whenever the keys were in use.
    const { keymap } = buildKeymap({ "toggle-zoom": "Ctrl+Shift+N" });
    expect(appActionFor(press("KeyN", { ctrlKey: true, shiftKey: true }), keymap)).toEqual({
      kind: "toggle-zoom",
    });
  });

  it("leaves an action on its default when the override cannot be used", () => {
    // Dropping it instead would be a silent way to lose a shortcut.
    const { keymap, problems } = buildKeymap({ "close-pane": "K" });
    expect(problems).toHaveLength(1);
    expect(problems[0]).toContain("close-pane");
    expect(appActionFor(press("KeyW", { ctrlKey: true, shiftKey: true }), keymap)).toEqual({
      kind: "close-pane",
    });
  });

  it("reports an action it does not have", () => {
    const { problems } = buildKeymap({ "make-coffee": "Ctrl+Shift+Z" });
    expect(problems).toHaveLength(1);
    expect(problems[0]).toContain("make-coffee");
  });

  it("changes nothing when there are none", () => {
    const { keymap, problems } = buildKeymap({});
    expect(problems).toEqual([]);
    expect(keymap).toEqual(shipped);
  });

  it("can be read back per action, for showing beside a command", () => {
    const { keymap } = buildKeymap({ "new-workspace": "Ctrl+Alt+T" });
    const shown = bindingsByAction(keymap);
    expect(shown.get("new-workspace")).toBe("Ctrl+Alt+T");
    expect(shown.get("close-pane")).toBe("Ctrl+Shift+W");
    expect(shown.get("switch-workspace-3")).toBe("Ctrl+3");
    expect(shown.has("open-diff")).toBe(false);
    expect(shown.size).toBe(Object.keys(DEFAULT_BINDINGS).length);
  });

  it("will bind onto a terminal key if that is what was asked for", () => {
    // Dex does not ship one, but it is the owner's terminal.
    const { keymap, problems } = buildKeymap({ "close-pane": "Ctrl+D" });
    expect(problems).toEqual([]);
    expect(appActionFor(press("KeyD", { ctrlKey: true }), keymap)).toEqual({ kind: "close-pane" });
  });
});
