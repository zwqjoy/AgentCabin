/**
 * AgentCabin Computer Use V3 macOS Curated Eval Cases (25 cases).
 *
 * Covers Calculator (5), TextEdit (5), Finder (5), Chrome (5), and Cross-App (5).
 */

export const MACOS_EVAL_CASES = [
  // --- Category: Calculator (5 cases) ---
  {
    id: "calc_01_simple_addition",
    title: "Simple Addition (12 + 30 = 42)",
    category: "calculator",
    targetApp: "Calculator",
    prompt: "Open Calculator, calculate 12 + 30, and verify the display shows 42.",
    initialState: {
      roots: [{ appName: "Calculator", title: "Calculator", pid: 101, windowId: 1 }],
      elements: [
        { role: "button", title: "1", ref: "@e1" },
        { role: "button", title: "2", ref: "@e2" },
        { role: "button", title: "+", ref: "@e3" },
        { role: "button", title: "3", ref: "@e4" },
        { role: "button", title: "0", ref: "@e5" },
        { role: "button", title: "=", ref: "@e6" },
        { role: "text", title: "Display", value: "0", ref: "@e7" },
      ],
    },
    actions: [
      { action: "click", ref: "@e1" },
      { action: "click", ref: "@e2" },
      { action: "click", ref: "@e3" },
      { action: "click", ref: "@e4" },
      { action: "click", ref: "@e5" },
      { action: "click", ref: "@e6" },
    ],
    groundTruthOutcome: { expect: { text: "42" } },
  },
  {
    id: "calc_02_clear_entry",
    title: "Clear Entry (Enter 99, press C, verify 0)",
    category: "calculator",
    targetApp: "Calculator",
    prompt: "Enter 99 into Calculator, click Clear, and verify the display resets to 0.",
    initialState: {
      roots: [{ appName: "Calculator", title: "Calculator", pid: 101, windowId: 1 }],
      elements: [
        { role: "button", title: "9", ref: "@e1" },
        { role: "button", title: "C", ref: "@e2" },
        { role: "text", title: "Display", value: "0", ref: "@e3" },
      ],
    },
    actions: [
      { action: "click", ref: "@e1" },
      { action: "click", ref: "@e1" },
      { action: "click", ref: "@e2" },
    ],
    groundTruthOutcome: { expect: { text: "0" } },
  },
  {
    id: "calc_03_multiplication",
    title: "Multiplication (7 * 8 = 56)",
    category: "calculator",
    targetApp: "Calculator",
    prompt: "Multiply 7 by 8 in Calculator and verify the result is 56.",
    initialState: {
      roots: [{ appName: "Calculator", title: "Calculator", pid: 101, windowId: 1 }],
      elements: [
        { role: "button", title: "7", ref: "@e1" },
        { role: "button", title: "×", ref: "@e2" },
        { role: "button", title: "8", ref: "@e3" },
        { role: "button", title: "=", ref: "@e4" },
        { role: "text", title: "Display", value: "0", ref: "@e5" },
      ],
    },
    actions: [
      { action: "click", ref: "@e1" },
      { action: "click", ref: "@e2" },
      { action: "click", ref: "@e3" },
      { action: "click", ref: "@e4" },
    ],
    groundTruthOutcome: { expect: { text: "56" } },
  },
  {
    id: "calc_04_decimal_calculation",
    title: "Decimal calculation (3.5 * 2 = 7)",
    category: "calculator",
    targetApp: "Calculator",
    prompt: "Calculate 3.5 * 2 in Calculator and verify the result is 7.",
    initialState: {
      roots: [{ appName: "Calculator", title: "Calculator", pid: 101, windowId: 1 }],
      elements: [
        { role: "button", title: "3", ref: "@e1" },
        { role: "button", title: ".", ref: "@e2" },
        { role: "button", title: "5", ref: "@e3" },
        { role: "button", title: "×", ref: "@e4" },
        { role: "button", title: "2", ref: "@e5" },
        { role: "button", title: "=", ref: "@e6" },
        { role: "text", title: "Display", value: "0", ref: "@e7" },
      ],
    },
    actions: [
      { action: "click", ref: "@e1" },
      { action: "click", ref: "@e2" },
      { action: "click", ref: "@e3" },
      { action: "click", ref: "@e4" },
      { action: "click", ref: "@e5" },
      { action: "click", ref: "@e6" },
    ],
    groundTruthOutcome: { expect: { text: "7" } },
  },
  {
    id: "calc_05_division_by_zero",
    title: "Division by zero (5 / 0 = Error)",
    category: "calculator",
    targetApp: "Calculator",
    prompt: "Divide 5 by 0 in Calculator and verify Error is shown.",
    initialState: {
      roots: [{ appName: "Calculator", title: "Calculator", pid: 101, windowId: 1 }],
      elements: [
        { role: "button", title: "5", ref: "@e1" },
        { role: "button", title: "÷", ref: "@e2" },
        { role: "button", title: "0", ref: "@e3" },
        { role: "button", title: "=", ref: "@e4" },
        { role: "text", title: "Display", value: "0", ref: "@e5" },
      ],
    },
    actions: [
      { action: "click", ref: "@e1" },
      { action: "click", ref: "@e2" },
      { action: "click", ref: "@e3" },
      { action: "click", ref: "@e4" },
    ],
    groundTruthOutcome: { expect: { text: "Error" } },
  },

  // --- Category: TextEdit (5 cases) ---
  {
    id: "textedit_01_type_sentence",
    title: "Type Sentence in Document",
    category: "textedit",
    targetApp: "TextEdit",
    prompt: "Type 'Hello world from AgentCabin' into TextEdit.",
    initialState: {
      roots: [{ appName: "TextEdit", title: "Untitled", pid: 201, windowId: 1 }],
      elements: [
        { role: "textarea", title: "Document Area", value: "", ref: "@e1" },
      ],
    },
    actions: [
      { action: "typeText", ref: "@e1", text: "Hello world from AgentCabin" },
    ],
    groundTruthOutcome: { expect: { text: "Hello world from AgentCabin" } },
  },
  {
    id: "textedit_02_read_text_content",
    title: "Read Multiline Document Content",
    category: "textedit",
    targetApp: "TextEdit",
    prompt: "Inspect and read the text document content in TextEdit.",
    initialState: {
      roots: [{ appName: "TextEdit", title: "Notes.txt", pid: 201, windowId: 1 }],
      elements: [
        { role: "textarea", title: "Document Area", value: "Line 1: Spec\nLine 2: Design", ref: "@e1" },
      ],
    },
    actions: [],
    groundTruthOutcome: { expect: { text: "Line 1: Spec" } },
  },
  {
    id: "textedit_03_replace_text",
    title: "Replace Text in Document",
    category: "textedit",
    targetApp: "TextEdit",
    prompt: "Replace the text in TextEdit with 'Updated Content'.",
    initialState: {
      roots: [{ appName: "TextEdit", title: "Untitled", pid: 201, windowId: 1 }],
      elements: [
        { role: "textarea", title: "Document Area", value: "Old Content", ref: "@e1" },
      ],
    },
    actions: [
      { action: "setText", ref: "@e1", text: "Updated Content" },
    ],
    groundTruthOutcome: { expect: { text: "Updated Content" } },
  },
  {
    id: "textedit_04_save_dialog_detection",
    title: "Trigger Save Sheet and Detect It",
    category: "textedit",
    targetApp: "TextEdit",
    prompt: "Trigger Cmd+S in TextEdit and wait for the save sheet to appear.",
    initialState: {
      roots: [{ appName: "TextEdit", title: "Untitled", pid: 201, windowId: 1 }],
      elements: [
        { role: "textarea", title: "Document Area", value: "Draft", ref: "@e1" },
      ],
    },
    actions: [
      { action: "keypress", keys: ["Meta", "s"] },
    ],
    groundTruthOutcome: { expect: { role: "sheet" } },
  },
  {
    id: "textedit_05_font_panel",
    title: "Expand Font Panel Controls",
    category: "textedit",
    targetApp: "TextEdit",
    prompt: "Open Font panel (Cmd+T) in TextEdit and verify font family picker is present.",
    initialState: {
      roots: [{ appName: "TextEdit", title: "Untitled", pid: 201, windowId: 1 }],
      elements: [
        { role: "textarea", title: "Document Area", value: "Font sample", ref: "@e1" },
      ],
    },
    actions: [
      { action: "keypress", keys: ["Meta", "t"] },
    ],
    groundTruthOutcome: { expect: { text: "Font" } },
  },

  // --- Category: Finder (5 cases) ---
  {
    id: "finder_01_navigate_documents",
    title: "Navigate to Documents Folder",
    category: "finder",
    targetApp: "Finder",
    prompt: "In Finder, click Documents in the sidebar and verify Documents folder opens.",
    initialState: {
      roots: [{ appName: "Finder", title: "Macintosh HD", pid: 301, windowId: 1 }],
      elements: [
        { role: "button", title: "Documents", ref: "@e1" },
        { role: "text", title: "Path Bar", value: "Macintosh HD", ref: "@e2" },
      ],
    },
    actions: [
      { action: "click", ref: "@e1" },
    ],
    groundTruthOutcome: { expect: { text: "Documents" } },
  },
  {
    id: "finder_02_search_file",
    title: "Search for File in Folder",
    category: "finder",
    targetApp: "Finder",
    prompt: "Search for 'README' in the Finder search field.",
    initialState: {
      roots: [{ appName: "Finder", title: "Documents", pid: 301, windowId: 1 }],
      elements: [
        { role: "input", title: "Search", value: "", ref: "@e1" },
      ],
    },
    actions: [
      { action: "typeText", ref: "@e1", text: "README" },
      { action: "keypress", keys: ["Enter"] },
    ],
    groundTruthOutcome: { expect: { text: "README" } },
  },
  {
    id: "finder_03_list_view_toggle",
    title: "Toggle Finder to List View",
    category: "finder",
    targetApp: "Finder",
    prompt: "Switch Finder view to List mode (Cmd+2).",
    initialState: {
      roots: [{ appName: "Finder", title: "Downloads", pid: 301, windowId: 1 }],
      elements: [
        { role: "segmented_control", title: "View Switcher", value: "Icons", ref: "@e1" },
      ],
    },
    actions: [
      { action: "keypress", keys: ["Meta", "2"] },
    ],
    groundTruthOutcome: { expect: { text: "List" } },
  },
  {
    id: "finder_04_select_multiple_items",
    title: "Select Multiple Items in Finder List",
    category: "finder",
    targetApp: "Finder",
    prompt: "Click the first file then click the second file with Shift modifier.",
    initialState: {
      roots: [{ appName: "Finder", title: "Projects", pid: 301, windowId: 1 }],
      elements: [
        { role: "row", title: "file_a.txt", ref: "@e1" },
        { role: "row", title: "file_b.txt", ref: "@e2" },
      ],
    },
    actions: [
      { action: "click", ref: "@e1" },
      { action: "click", ref: "@e2" },
    ],
    groundTruthOutcome: { expect: { ref: "@e2" } },
  },
  {
    id: "finder_05_inspect_status_bar",
    title: "Inspect Item Count in Status Bar",
    category: "finder",
    targetApp: "Finder",
    prompt: "Inspect the status bar in Finder to read the total item count.",
    initialState: {
      roots: [{ appName: "Finder", title: "Downloads", pid: 301, windowId: 1 }],
      elements: [
        { role: "text", title: "Status Bar", value: "14 items, 82 GB available", ref: "@e1" },
      ],
    },
    actions: [],
    groundTruthOutcome: { expect: { text: "14 items" } },
  },

  // --- Category: Chrome (5 cases) ---
  {
    id: "chrome_01_navigate_url",
    title: "Navigate to Target URL in Chrome",
    category: "chrome",
    targetApp: "Google Chrome",
    prompt: "Navigate Chrome to https://example.com and verify page loads.",
    initialState: {
      roots: [{ appName: "Google Chrome", title: "New Tab", backend: "cdp", browserTargetId: "tab-1" }],
      elements: [
        { role: "input", title: "Address and search bar", value: "about:blank", ref: "@e1" },
      ],
    },
    actions: [
      { action: "typeText", ref: "@e1", text: "https://example.com" },
      { action: "keypress", keys: ["Enter"] },
    ],
    groundTruthOutcome: { expect: { text: "Example Domain" } },
  },
  {
    id: "chrome_02_click_link",
    title: "Click Link in Web Page",
    category: "chrome",
    targetApp: "Google Chrome",
    prompt: "Click 'More information...' link in Chrome.",
    initialState: {
      roots: [{ appName: "Google Chrome", title: "Example Domain", backend: "cdp", browserTargetId: "tab-1" }],
      elements: [
        { role: "link", title: "More information...", ref: "@e1" },
      ],
    },
    actions: [
      { action: "click", ref: "@e1" },
    ],
    groundTruthOutcome: { expect: { text: "IANA" } },
  },
  {
    id: "chrome_03_input_form",
    title: "Fill Input Field and Submit Form",
    category: "chrome",
    targetApp: "Google Chrome",
    prompt: "Type 'AgentCabin' into search box and submit.",
    initialState: {
      roots: [{ appName: "Google Chrome", title: "Search Engine", backend: "cdp", browserTargetId: "tab-1" }],
      elements: [
        { role: "input", title: "Search query", value: "", ref: "@e1" },
        { role: "button", title: "Search", ref: "@e2" },
      ],
    },
    actions: [
      { action: "setText", ref: "@e1", text: "AgentCabin" },
      { action: "click", ref: "@e2" },
    ],
    groundTruthOutcome: { expect: { text: "AgentCabin Results" } },
  },
  {
    id: "chrome_04_tab_switching",
    title: "Switch Between Browser Tabs",
    category: "chrome",
    targetApp: "Google Chrome",
    prompt: "Switch to tab 2 in Chrome.",
    initialState: {
      roots: [
        { appName: "Google Chrome", title: "Tab 1", backend: "cdp", browserTargetId: "0" },
        { appName: "Google Chrome", title: "Tab 2 - Dashboard", backend: "cdp", browserTargetId: "1" },
      ],
      elements: [{ role: "button", title: "Dashboard", ref: "@e1" }],
    },
    actions: [],
    groundTruthOutcome: { expect: { text: "Dashboard" } },
  },
  {
    id: "chrome_05_canvas_fallback",
    title: "Trigger Visual Grounding on HTML5 Canvas",
    category: "chrome",
    targetApp: "Google Chrome",
    prompt: "Interact with interactive button inside HTML5 Canvas.",
    initialState: {
      roots: [{ appName: "Google Chrome", title: "WebGL App", backend: "cdp", browserTargetId: "tab-1" }],
      elements: [
        { role: "canvas", title: "Main Game Canvas", ref: "@e1" },
      ],
    },
    actions: [
      { action: "click", x: 250, y: 150 },
    ],
    groundTruthOutcome: { expect: { requireVisualDiff: true } },
  },

  // --- Category: Cross-App (5 cases) ---
  {
    id: "cross_01_copy_calc_to_textedit",
    title: "Copy Calculator Result into TextEdit",
    category: "cross_app",
    targetApp: "Calculator",
    prompt: "Calculate result in Calculator and paste into TextEdit document.",
    initialState: {
      roots: [
        { appName: "Calculator", title: "Calculator", pid: 101, windowId: 1 },
        { appName: "TextEdit", title: "Untitled", pid: 201, windowId: 2 },
      ],
      elements: [
        { role: "text", title: "Display", value: "42", ref: "@e1" },
      ],
    },
    actions: [
      { action: "keypress", keys: ["Meta", "c"] },
    ],
    groundTruthOutcome: { expect: { text: "42" } },
  },
  {
    id: "cross_02_finder_to_textedit",
    title: "Locate File in Finder and Open in TextEdit",
    category: "cross_app",
    targetApp: "Finder",
    prompt: "Double-click log file in Finder to launch TextEdit.",
    initialState: {
      roots: [
        { appName: "Finder", title: "Logs", pid: 301, windowId: 1 },
      ],
      elements: [
        { role: "row", title: "app.log", ref: "@e1" },
      ],
    },
    actions: [
      { action: "click", ref: "@e1", clickCount: 2 },
    ],
    groundTruthOutcome: { expect: { text: "app.log" } },
  },
  {
    id: "cross_03_chrome_to_textedit",
    title: "Copy Web Text from Chrome into TextEdit",
    category: "cross_app",
    targetApp: "Google Chrome",
    prompt: "Copy headline from Chrome web page and paste into TextEdit.",
    initialState: {
      roots: [
        { appName: "Google Chrome", title: "News Portal", backend: "cdp", browserTargetId: "tab-1" },
        { appName: "TextEdit", title: "Notes", pid: 201, windowId: 2 },
      ],
      elements: [
        { role: "heading", title: "Breaking News: AgentCabin V3 Released", ref: "@e1" },
      ],
    },
    actions: [],
    groundTruthOutcome: { expect: { text: "Breaking News" } },
  },
  {
    id: "cross_04_switch_app_focus",
    title: "Switch Focus Between Multiple Running Apps",
    category: "cross_app",
    targetApp: "Finder",
    prompt: "Switch focus between Finder and Calculator.",
    initialState: {
      roots: [
        { appName: "Finder", title: "Finder", pid: 301, windowId: 1 },
        { appName: "Calculator", title: "Calculator", pid: 101, windowId: 2 },
      ],
      elements: [],
    },
    actions: [],
    groundTruthOutcome: { expect: { text: "Calculator" } },
  },
  {
    id: "cross_05_multi_root_coordination",
    title: "Multi-Root UI Isolation across 3 Apps",
    category: "cross_app",
    targetApp: "Multiple",
    prompt: "Enumerate roots across Finder, TextEdit, and Chrome without state leakage.",
    initialState: {
      roots: [
        { appName: "Finder", title: "Finder", pid: 301, windowId: 1 },
        { appName: "TextEdit", title: "TextEdit", pid: 201, windowId: 2 },
        { appName: "Google Chrome", title: "Google Chrome", backend: "cdp", browserTargetId: "0" },
      ],
      elements: [],
    },
    actions: [],
    groundTruthOutcome: { expect: { role: "root" } },
  },
];
