import { describe, expect, it } from "vitest";
import {
  ACTIONS,
  DEFAULT_BINDINGS,
  appActionFor,
  bindingsByAction,
  buildKeymap,
  forPlatform,
  isMacUserAgent,
  parseBinding,
  titled,
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
    expect(unbound).toEqual(["open-diff", "open-activity", "view-terminal", "view-office", "cycle-view", "open-setup", "open-settings"]);
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

describe("on macOS", () => {
  const mac: Keymap = buildKeymap({}, true).keymap;

  it("the shipped bindings use Cmd where Windows uses Ctrl: Cmd never reaches the terminal", () => {
    expect(appActionFor(press("KeyN", { metaKey: true, shiftKey: true }), mac)).toEqual({ kind: "new-workspace" });
    expect(appActionFor(press("KeyP", { metaKey: true, shiftKey: true }), mac)).toEqual({ kind: "command-palette" });
    expect(appActionFor(press("Digit3", { metaKey: true }), mac)).toEqual({ kind: "switch-workspace", index: 2 });
    expect(appActionFor(press("KeyW", { metaKey: true, shiftKey: true }), mac)).toEqual({ kind: "close-pane" });
  });

  it("leaves every Ctrl combination to the terminal, where zsh and Claude Code use them", () => {
    expect(appActionFor(press("KeyN", { ctrlKey: true, shiftKey: true }), mac)).toBeNull();
    expect(appActionFor(press("KeyC", { ctrlKey: true }), mac)).toBeNull();
    expect(appActionFor(press("KeyR", { ctrlKey: true }), mac)).toBeNull();
  });

  it("moves between panes with Cmd+Option, leaving Option+Arrow's word jumps to the shell", () => {
    expect(appActionFor(press("ArrowLeft", { metaKey: true, altKey: true }), mac)).toEqual({
      kind: "focus-pane",
      direction: "left",
    });
    expect(appActionFor(press("ArrowLeft", { metaKey: true, altKey: true, shiftKey: true }), mac)).toEqual({
      kind: "move-pane",
      direction: "left",
    });
    expect(appActionFor(press("ArrowLeft", { altKey: true }), mac)).toBeNull();
    expect(appActionFor(press("ArrowRight", { altKey: true }), mac)).toBeNull();
  });

  it("leaves copy, paste and the rest of Cmd's everyday keys to the window", () => {
    for (const code of ["KeyC", "KeyV", "KeyX", "KeyA", "KeyQ", "KeyM", "KeyH"]) {
      expect(appActionFor(press(code, { metaKey: true }), mac), code).toBeNull();
    }
  });

  it("reads Cmd in a written binding, by any name a Mac user would write", () => {
    expect(parseBinding("cmd+k")).toEqual({ binding: "Cmd+K" });
    expect(parseBinding("Command+Shift+K")).toEqual({ binding: "Cmd+Shift+K" });
    expect(parseBinding("Shift+Meta+K")).toEqual({ binding: "Cmd+Shift+K" });
    expect(parseBinding("Ctrl+Cmd+K")).toEqual({ binding: "Ctrl+Cmd+K" });
  });

  it("lets the owner bind onto Cmd in their config", () => {
    const { keymap, problems } = buildKeymap({ "command-palette": "Cmd+K" }, true);
    expect(problems).toEqual([]);
    expect(appActionFor(press("KeyK", { metaKey: true }), keymap)).toEqual({ kind: "command-palette" });
  });
});

describe("forPlatform", () => {
  it("says a shipped binding the way the platform presses it", () => {
    expect(forPlatform("Ctrl+Shift+W", true)).toBe("Cmd+Shift+W");
    expect(forPlatform("Ctrl+1", true)).toBe("Cmd+1");
    expect(forPlatform("Alt+Shift+ArrowLeft", true)).toBe("Cmd+Alt+Shift+ArrowLeft");
    expect(forPlatform("Alt+ArrowUp", false)).toBe("Alt+ArrowUp");
    expect(forPlatform("Ctrl+Shift+W", false)).toBe("Ctrl+Shift+W");
  });
});

describe("isMacUserAgent", () => {
  it("knows a Mac from Windows by what the webview says it is", () => {
    expect(isMacUserAgent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15")).toBe(true);
    expect(isMacUserAgent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Edg/140.0")).toBe(false);
  });
});

describe("on Windows, the Windows key", () => {
  it("never runs a shipped binding, though Cmd bindings exist on a Mac", () => {
    const windows: Keymap = buildKeymap({}, false).keymap;
    expect(appActionFor(press("KeyP", { metaKey: true, ctrlKey: true, shiftKey: true }), windows)).toBeNull();
    expect(appActionFor(press("Digit1", { metaKey: true }), windows)).toBeNull();
  });
});

describe("an override that takes another action's key", () => {
  it("is reported, naming the action left without one", () => {
    const { keymap, problems } = buildKeymap({ "new-workspace": "Cmd+Shift+P" }, true);
    expect(appActionFor(press("KeyP", { metaKey: true, shiftKey: true }), keymap)).toEqual({ kind: "new-workspace" });
    expect(problems).toEqual([
      "keys.new-workspace takes Cmd+Shift+P from command-palette, which now has no key",
    ]);
  });

  it("is not reported when the other action was given a key of its own", () => {
    const { problems } = buildKeymap({ "new-workspace": "Ctrl+Shift+P", "command-palette": "Ctrl+Alt+P" }, false);
    expect(problems).toEqual([]);
  });

  it("is reported when two overrides name the same key", () => {
    const { problems } = buildKeymap({ "split-right": "Ctrl+Alt+K", "split-down": "Ctrl+Alt+K" }, false);
    expect(problems).toHaveLength(1);
    expect(problems[0]).toContain("Ctrl+Alt+K");
  });
});

describe("titled", () => {
  it("names the key in a tooltip only when the action has one", () => {
    expect(titled("Close pane", "Cmd+Shift+W")).toBe("Close pane (Cmd+Shift+W)");
    expect(titled("Close pane", undefined)).toBe("Close pane");
  });
});
