// picker_bridge.js — injected into preview window for element selection
// Communicates back via navigation intercept (agentcabin-bridge://)
(function() {
  if (window.__agentcabinPicker) return;

  var STYLE_KEYS = [
    'display','position','width','height','padding','margin',
    'fontSize','fontWeight','color','backgroundColor',
    'borderRadius','border','gap','flexDirection','alignItems',
    'justifyContent','opacity','overflow','zIndex'
  ];

  var active = false;
  var overlay = null;
  var labelEl = null;
  var pendingData = null;
  var toolbar = null;
  var resultPanel = null;
  var rafPending = false; // rAF gate for mousemove throttle

  var BTN = 'padding:5px 14px;border:none;border-radius:6px;cursor:pointer;font:500 12px/1 system-ui,sans-serif;';
  var BAR = 'position:fixed;bottom:0;left:0;right:0;z-index:1000001;' +
    'background:#1e1e2e;border-top:1px solid #45475a;font:12px/1.5 system-ui,sans-serif;';

  // ── Toolbar (always visible after injection) ──

  function createToolbar() {
    if (toolbar) return;
    var bar = document.createElement('div');
    bar.id = '__agentcabin_toolbar';
    bar.style.cssText = BAR + 'display:flex;align-items:center;padding:8px 16px;gap:10px;';
    bar.innerHTML =
      '<span style="color:#a6adc8;flex:1;font-size:11px;">AgentCabin Preview</span>' +
      '<button id="__agentcabin_btn_pick" style="' + BTN + 'background:#3b82f6;color:#fff;">Pick Element</button>';
    document.body.appendChild(bar);
    document.body.style.paddingBottom = (bar.offsetHeight) + 'px';
    toolbar = bar;

    document.getElementById('__agentcabin_btn_pick').onclick = function() {
      activate();
    };
  }

  function setToolbarPicking(isPicking) {
    var btn = document.getElementById('__agentcabin_btn_pick');
    if (!btn) return;
    if (isPicking) {
      btn.textContent = 'Cancel';
      btn.style.background = '#45475a';
      btn.style.color = '#cdd6f4';
      btn.onclick = function() { deactivate(); setToolbarPicking(false); };
    } else {
      btn.textContent = 'Pick Element';
      btn.style.background = '#3b82f6';
      btn.style.color = '#fff';
      btn.onclick = function() { activate(); };
    }
  }

  // ── Hover overlay ──

  function createOverlay() {
    // Guard against duplicate overlay elements
    var existing = document.getElementById('__agentcabin_overlay');
    if (existing) existing.remove();
    var existingLb = document.getElementById('__agentcabin_label');
    if (existingLb) existingLb.remove();

    var el = document.createElement('div');
    el.id = '__agentcabin_overlay';
    el.style.cssText = 'position:fixed;pointer-events:none;border:2px solid #3b82f6;' +
      'background:rgba(59,130,246,0.08);z-index:999999;transition:all 0.1s ease;display:none;';
    document.body.appendChild(el);

    var lb = document.createElement('div');
    lb.id = '__agentcabin_label';
    lb.style.cssText = 'position:fixed;pointer-events:none;z-index:1000000;' +
      'background:#3b82f6;color:#fff;font:11px/1.4 monospace;padding:2px 6px;' +
      'border-radius:3px;display:none;white-space:nowrap;';
    document.body.appendChild(lb);

    return { overlay: el, label: lb };
  }

  function highlight(el) {
    if (!overlay) return;
    var r = el.getBoundingClientRect();
    overlay.style.display = 'block';
    overlay.style.left = r.left + 'px';
    overlay.style.top = r.top + 'px';
    overlay.style.width = r.width + 'px';
    overlay.style.height = r.height + 'px';

    var tag = el.tagName.toLowerCase();
    if (el.id) tag += '#' + el.id;
    else if (el.className && typeof el.className === 'string')
      tag += '.' + el.className.trim().split(/\s+/)[0];
    labelEl.textContent = tag;
    labelEl.style.display = 'block';
    labelEl.style.left = r.left + 'px';
    labelEl.style.top = Math.max(0, r.top - 22) + 'px';
  }

  function hideOverlay() {
    if (overlay) { overlay.style.display = 'none'; labelEl.style.display = 'none'; }
  }

  // ── Data extraction ──

  function getDomPath(el) {
    var parts = [];
    var cur = el;
    while (cur && cur !== document.body && cur !== document.documentElement) {
      var sel = cur.tagName.toLowerCase();
      if (cur.id) sel += '#' + cur.id;
      else if (cur.className && typeof cur.className === 'string')
        sel += '.' + cur.className.trim().split(/\s+/).join('.');
      parts.unshift(sel);
      cur = cur.parentElement;
    }
    return 'body > ' + parts.join(' > ');
  }

  function getKeyStyles(el) {
    var cs = getComputedStyle(el);
    var result = {};
    for (var i = 0; i < STYLE_KEYS.length; i++) {
      var k = STYLE_KEYS[i];
      var v = cs[k];
      if (v && v !== 'none' && v !== 'normal' && v !== 'auto' && v !== '0px' && v !== '0') {
        result[k] = v;
      }
    }
    return result;
  }

  function extract(el) {
    return {
      url: location.href,
      viewport: { width: innerWidth, height: innerHeight },
      domPath: getDomPath(el),
      tagName: el.tagName.toLowerCase(),
      textContent: (el.textContent || '').trim().slice(0, 500),
      attributes: {
        id: el.id || null,
        class: (typeof el.className === 'string' ? el.className : null),
        role: el.getAttribute('role'),
        name: el.getAttribute('name'),
        ariaLabel: el.getAttribute('aria-label'),
      },
      outerHtmlSnippet: el.outerHTML.slice(0, 2000),
      styleSummary: getKeyStyles(el),
    };
  }

  // ── Result panel (replaces toolbar after selection) ──

  function esc(s) {
    return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
  }

  function dismissResult() {
    removeResult();
    pendingData = null;
    if (toolbar) toolbar.style.display = 'flex';
  }

  function showResult(data) {
    removeResult();
    pendingData = data;
    if (toolbar) toolbar.style.display = 'none';

    var tag = esc(data.tagName);
    var cls = data.attributes.class ? '.' + esc(data.attributes.class.split(' ')[0]) : '';
    var text = data.textContent ? esc(data.textContent.slice(0, 80)) : '';

    var p = document.createElement('div');
    p.id = '__agentcabin_result';
    p.style.cssText = BAR + 'padding:10px 16px;';
    p.innerHTML =
      '<div style="display:flex;align-items:center;gap:8px;margin-bottom:8px;">' +
        '<span style="font:600 13px monospace;color:#89b4fa;">' + tag + cls + '</span>' +
        (text ? '<span style="color:#a6adc8;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;flex:1;">"' + text + '"</span>' : '') +
      '</div>' +
      '<div style="display:flex;gap:8px;justify-content:flex-end;">' +
        '<button id="__agentcabin_r_pick" style="' + BTN + 'background:#45475a;color:#cdd6f4;">Pick Again</button>' +
        '<button id="__agentcabin_r_dismiss" style="' + BTN + 'background:#45475a;color:#cdd6f4;">Dismiss</button>' +
        '<button id="__agentcabin_r_insert" style="' + BTN + 'background:#3b82f6;color:#fff;">Insert to Chat</button>' +
      '</div>';
    document.body.appendChild(p);
    resultPanel = p;

    document.getElementById('__agentcabin_r_insert').onclick = function() {
      if (!pendingData) return;
      var json = encodeURIComponent(JSON.stringify(pendingData));
      dismissResult();
      window.location.href = 'agentcabin-bridge://element-selected#' + json;
    };
    document.getElementById('__agentcabin_r_pick').onclick = function() {
      dismissResult();
      activate();
    };
    document.getElementById('__agentcabin_r_dismiss').onclick = dismissResult;
  }

  function removeResult() {
    var el = document.getElementById('__agentcabin_result');
    if (el) el.remove();
    resultPanel = null;
  }

  // ── Picker logic ──

  function isAgentCabinEl(el) {
    if (!el) return false;
    return el.closest('#__agentcabin_toolbar') || el.closest('#__agentcabin_result') ||
      el.id === '__agentcabin_overlay' || el.id === '__agentcabin_label';
  }

  function onMove(e) {
    if (!active || rafPending) return;
    rafPending = true;
    var x = e.clientX, y = e.clientY;
    requestAnimationFrame(function() {
      rafPending = false;
      if (!active) return;
      var el = document.elementFromPoint(x, y);
      if (el && !isAgentCabinEl(el)) highlight(el);
    });
  }

  function onClick(e) {
    if (!active) return;
    var el = document.elementFromPoint(e.clientX, e.clientY);
    if (!el || isAgentCabinEl(el)) return;
    e.preventDefault();
    e.stopPropagation();
    e.stopImmediatePropagation();
    deactivate();
    var data = extract(el);
    showResult(data);
  }

  function activate() {
    if (active) return;
    removeResult();
    pendingData = null;
    active = true;
    var els = createOverlay();
    overlay = els.overlay;
    labelEl = els.label;
    document.body.style.cursor = 'crosshair';
    document.addEventListener('mousemove', onMove, true);
    document.addEventListener('click', onClick, true);
    setToolbarPicking(true);
  }

  function deactivate() {
    active = false;
    rafPending = false;
    document.body.style.cursor = '';
    document.removeEventListener('mousemove', onMove, true);
    document.removeEventListener('click', onClick, true);
    hideOverlay();
    var ov = document.getElementById('__agentcabin_overlay');
    if (ov) ov.remove();
    var lb = document.getElementById('__agentcabin_label');
    if (lb) lb.remove();
    overlay = null;
    labelEl = null;
    setToolbarPicking(false);
  }

  // ── Init: show toolbar as soon as body exists ──
  function init() {
    if (document.body) {
      createToolbar();
    } else {
      document.addEventListener('DOMContentLoaded', createToolbar);
    }
  }
  init();

  window.__agentcabinPicker = { activate: activate, deactivate: deactivate };
})();
