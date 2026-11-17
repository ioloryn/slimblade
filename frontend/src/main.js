// Material 3 settings UI for SlimBlade, powered by Tauri IPC.
import '@material/web/button/filled-button.js';
import '@material/web/button/filled-tonal-button.js';
import '@material/web/button/outlined-button.js';
import '@material/web/button/text-button.js';
import '@material/web/checkbox/checkbox.js';
import '@material/web/slider/slider.js';
import '@material/web/select/outlined-select.js';
import '@material/web/select/select-option.js';
import '@material/web/textfield/outlined-text-field.js';
import '@material/web/icon/icon.js';
import '@material/web/iconbutton/icon-button.js';
import '@material/web/labs/card/elevated-card.js';
import '@material/web/dialog/dialog.js';
import { invoke } from '@tauri-apps/api/core';
import './styles.css';

const root = document.getElementById('app');

let state = null;
let dirty = false;
let selectedButton = null;
let notice = null;

const esc = (value) =>
  String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');

const KINDS = [
  ['mouse', 'Кнопка мыши'],
  ['keys', 'Сочетание клавиш'],
  ['drag_lock', 'Drag lock'],
  ['exec', 'Команда'],
  ['none', 'Отключена'],
];

function actionToForm(action) {
  if (!action) return { kind: 'none', value: '' };
  switch (action.action) {
    case 'mouse':
    case 'drag_lock':
      return { kind: action.action, value: action.button ?? 'left' };
    case 'keys':
      return { kind: 'keys', value: (action.keys ?? []).join(', ') };
    case 'exec':
      return { kind: 'exec', value: action.command ?? '' };
    default:
      return { kind: 'none', value: '' };
  }
}

function formToAction(kind, value) {
  const v = String(value ?? '').trim();
  switch (kind) {
    case 'mouse':
      return { action: 'mouse', button: v || 'left' };
    case 'drag_lock':
      return { action: 'drag_lock', button: v || 'left' };
    case 'keys':
      return {
        action: 'keys',
        keys: v ? v.split(/[,\s+]+/).filter(Boolean) : [],
      };
    case 'exec':
      return { action: 'exec', command: v };
    default:
      return { action: 'none' };
  }
}

function statusClass(ok) {
  return ok === true ? 'status-ok' : ok === false ? 'status-bad' : 'status-wait';
}

function statusHtml(s) {
  if (!s) return '<span class="muted">—</span>';
  return `<span class="${statusClass(s.ok)}">${esc(s.text)}</span>`;
}

function buttonLabel(name) {
  const map = {
    left: 'Левая кнопка',
    primary: 'Левая кнопка',
    right: 'Правая кнопка',
    secondary: 'Правая кнопка',
    middle: 'Средняя кнопка',
    side: 'Боковая кнопка',
    back: 'Боковая кнопка',
    extra: 'Дополнительная кнопка',
    forward: 'Дополнительная кнопка',
  };
  return map[name.toLowerCase()] ?? name;
}

function trackballSvg() {
  // Stylised Kensington SlimBlade: near-square dark body, red ball in a
  // recessed bezel, four quadrant buttons and the original proportions.
  const zones = [
    ['middle', 'Средняя', 40, 48, 'start'],
    ['side', 'Боковая', 360, 48, 'end'],
    ['left', 'Левая', 40, 300, 'start'],
    ['right', 'Правая', 360, 300, 'end'],
  ];
  const zoneHtml = zones
    .map(([id, label, x, y, anchor], index) => {
      const form = actionToForm(state.config.buttons[id]);
      const summary =
        form.kind === 'none' ? 'откл.' : form.value || KINDS.find((k) => k[0] === form.kind)[1];
      const active = selectedButton === id;
      const rectX = index % 2 === 0 ? 24 : 216;
      const rectY = index < 2 ? 24 : 248;
      return `<g class="tb-zone ${active ? 'selected' : ''}" data-button="${id}">
        <rect x="${rectX}" y="${rectY}" width="160" height="158" rx="10" />
        <text x="${x}" y="${y}" text-anchor="${anchor}" class="tb-label">${label}</text>
        <text x="${x}" y="${y + 16}" text-anchor="${anchor}" class="tb-summary">${esc(summary)}</text>
      </g>`;
    })
    .join('');

  const running = state.statuses.daemon?.ok === true;
  return `<svg class="trackball" viewBox="0 0 400 440" role="img" aria-label="Kensington SlimBlade — схема кнопок">
    <defs>
      <linearGradient id="tb-body" x1="0" y1="0" x2="1" y2="1">
        <stop offset="0" stop-color="#3a3a3d" />
        <stop offset="0.5" stop-color="#262629" />
        <stop offset="1" stop-color="#1c1c1f" />
      </linearGradient>
      <linearGradient id="tb-bezel" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0" stop-color="#5a5a5e" />
        <stop offset="0.45" stop-color="#333336" />
        <stop offset="1" stop-color="#171719" />
      </linearGradient>
      <radialGradient id="tb-ball" cx="38%" cy="32%" r="72%">
        <stop offset="0" stop-color="${running ? '#ff6b64' : '#c45a58'}" />
        <stop offset="0.45" stop-color="${running ? '#c41f24' : '#8c2024'}" />
        <stop offset="1" stop-color="${running ? '#5e0a10' : '#4a1013'}" />
      </radialGradient>
    </defs>

    <!-- cable -->
    <path d="M186 0 h28 v16 h-28 z" fill="#161618" />
    <!-- body -->
    <rect x="16" y="8" width="368" height="418" rx="16"
      fill="url(#tb-body)" stroke="#0d0d0e" stroke-width="2" />
    <!-- quadrant seams -->
    <line x1="200" y1="16" x2="200" y2="420" stroke="#4a4a4e" stroke-width="1" opacity="0.55" />
    <line x1="22" y1="196" x2="378" y2="196" stroke="#4a4a4e" stroke-width="1" opacity="0.55" />

    ${zoneHtml}

    <!-- ball assembly (does not intercept clicks) -->
    <g pointer-events="none">
      <circle cx="200" cy="196" r="108" fill="#101011" opacity="0.55" />
      <circle cx="200" cy="194" r="102" fill="url(#tb-bezel)" stroke="#0b0b0c" stroke-width="2" />
      <circle cx="200" cy="194" r="90" fill="#151517" stroke="#55555a" stroke-width="1.5" />
      <circle cx="200" cy="194" r="82" fill="url(#tb-ball)" stroke="#3a080c" stroke-width="2" />
      <ellipse cx="200" cy="152" rx="54" ry="17" fill="#ffffff" opacity="0.28" />
      <path d="M150 178 Q200 138 250 178 Q200 158 150 178 Z" fill="#ffd9d6" opacity="0.35" />
      <circle cx="228" cy="222" r="26" fill="#000000" opacity="0.12" />
    </g>

    <!-- logo -->
    <text x="200" y="404" text-anchor="middle" class="tb-logo">✦ Kensington</text>
  </svg>`;
}

function render() {
  const s = state;
  const dark = localStorage.getItem('slimblade-theme') === 'dark';
  document.documentElement.dataset.theme = dark ? 'dark' : 'light';
  const zoom = Number(localStorage.getItem('slimblade-zoom') ?? 100);
  document.documentElement.style.zoom = String(zoom / 100);

  const noticeHtml = notice
    ? `<div class="notice ${notice.kind === 'error' ? 'error' : ''}">${esc(notice.text)}</div>`
    : '';

  root.innerHTML = `
    <header class="app-bar">
      <h1>SlimBlade</h1>
      <span class="subtitle">Kensington SlimBlade Trackball</span>
      <span class="spacer"></span>
      <div class="toolbar-actions">
        <md-filled-button id="save" ${dirty ? '' : 'disabled'}>Сохранить</md-filled-button>
        <md-filled-tonal-button id="apply">Применить</md-filled-tonal-button>
        <md-outlined-button id="daemon">${s.statuses.daemon?.ok ? 'Остановить демон' : 'Запустить демон'}</md-outlined-button>
        <md-text-button id="hide">В трей</md-text-button>
        <md-text-button id="quit">Выход</md-text-button>
        <div class="zoom">
          <span>Масштаб</span>
          <md-icon-button id="zoom-out"><md-icon>−</md-icon></md-icon-button>
          <span>${Math.round(zoom)}%</span>
          <md-icon-button id="zoom-in"><md-icon>+</md-icon></md-icon-button>
        </div>
        <md-icon-button id="theme" title="Светлая / тёмная тема">
          <md-icon>${dark ? '☀' : '☾'}</md-icon>
        </md-icon-button>
      </div>
    </header>

    <main class="content">
      ${noticeHtml}

      <md-elevated-card>
        <div class="card-content">
          <h2 class="card-title">Состояние</h2>
          <div class="status-grid" id="status-grid">${statusRowsHtml()}</div>
        </div>
      </md-elevated-card>

      <md-elevated-card>
        <div class="card-content">
          <h2 class="card-title">Прокрутка</h2>
          <div class="scroll-row">
            <md-checkbox id="natural" ${s.config.scroll.natural ? 'checked' : ''}></md-checkbox>
            <label for="natural" class="check-label">Инвертировать</label>
            <span class="v-divider"></span>
            <span class="slider-label">Скорость</span>
            <md-slider id="speed" min="0.1" max="10" step="0.1"
              value="${s.config.scroll.speed}"></md-slider>
            <span class="slider-value" id="speed-value">${Number(s.config.scroll.speed).toFixed(1)}×</span>
            <md-text-button id="speed-reset">1.0×</md-text-button>
          </div>
          <div class="muted">Системная инверсия применяется поверх этого значения.</div>
        </div>
      </md-elevated-card>

      <md-elevated-card>
        <div class="card-content">
          <h2 class="card-title">Кнопки</h2>
          <div class="muted">Нажмите на кнопку на схеме, чтобы назначить действие.</div>
          <div class="trackball-wrap" id="trackball">${trackballSvg()}</div>
        </div>
      </md-elevated-card>

      <md-elevated-card>
        <div class="card-content">
          <h2 class="card-title">Устройство</h2>
          <div class="field-grid">
            <md-outlined-text-field data-device="vendor" label="Vendor" value="${esc(hexId(s.config.device.vendor))}" placeholder="0x047d"></md-outlined-text-field>
            <md-outlined-text-field data-device="product" label="Product" value="${esc(hexId(s.config.device.product))}" placeholder="0x2041"></md-outlined-text-field>
            <md-outlined-text-field data-device="name" label="Имя содержит" value="${esc(s.config.device.name ?? '')}" placeholder="Slimblade"></md-outlined-text-field>
            <md-outlined-text-field data-device="path" label="Event path" value="${esc(s.config.device.path ?? '')}" placeholder="/dev/input/event17"></md-outlined-text-field>
          </div>
          <div class="row">
            <md-outlined-button id="device-default">Фильтры по умолчанию</md-outlined-button>
            <md-text-button id="refresh">Проверить сейчас</md-text-button>
          </div>
        </div>
      </md-elevated-card>

      <md-elevated-card>
        <div class="card-content">
          <h2 class="card-title">Журнал</h2>
          <pre class="log-view" id="logs">${esc((state.logs ?? []).join('\n'))}</pre>
          <div><md-text-button id="clear-logs">Очистить</md-text-button></div>
        </div>
      </md-elevated-card>

      <div class="muted">Сохранение из GUI перезаписывает config.toml без комментариев. Для ручного редактирования используйте CLI и SIGHUP.</div>
    </main>

    <md-dialog id="button-dialog">
      <div slot="headline" id="dlg-title">Кнопка</div>
      <div slot="content">
        <div class="dialog-fields">
          <md-outlined-select label="Действие" id="dlg-kind">
            ${KINDS.map(
              ([value, label]) =>
                `<md-select-option value="${value}">${label}</md-select-option>`,
            ).join('')}
          </md-outlined-select>
          <md-outlined-text-field label="Значение" id="dlg-value"></md-outlined-text-field>
          <div class="muted" id="dlg-hint"></div>
        </div>
      </div>
      <div slot="actions">
        <md-filled-button id="dlg-close">Готово</md-filled-button>
      </div>
    </md-dialog>`;

  bindEvents();
}

function hexId(value) {
  return value == null ? '' : `0x${Number(value).toString(16).padStart(4, '0')}`;
}

function statusRowsHtml() {
  const s = state.statuses;
  const rows = [
    ['Трекбол', s.device],
    ['Виртуальный указатель', s.virtual_device],
    ['Демон', s.daemon],
    ['udev-правило', s.udev],
    ['Трей', s.tray],
  ];
  return rows
    .map(([label, value]) => `<span class="label">${label}</span><span class="value">${statusHtml(value)}</span>`)
    .join('') +
    `<span class="label">Конфигурация</span><span class="value ${statusClass(!dirty)}">${esc(state.config_path)}${dirty ? ' (не сохранена)' : ''}</span>`;
}

function refreshDynamic() {
  const grid = document.getElementById('status-grid');
  if (grid) grid.innerHTML = statusRowsHtml();
  const logs = document.getElementById('logs');
  if (logs) logs.textContent = (state.logs ?? []).join('\n');
  const daemon = document.getElementById('daemon');
  if (daemon) daemon.textContent = state.statuses.daemon?.ok ? 'Остановить демон' : 'Запустить демон';
  const save = document.getElementById('save');
  if (save) save.disabled = !dirty;
}

function markDirty() {
  dirty = true;
  refreshDynamic();
}

function bindEvents() {
  const byId = (id) => document.getElementById(id);

  byId('save').addEventListener('click', () => call('save_config', { config: state.config }));
  byId('apply').addEventListener('click', () => call('apply_config', { config: state.config }));
  byId('daemon').addEventListener('click', () => call('daemon', { cmd: 'toggle' }));
  byId('hide').addEventListener('click', () => call('hide_to_tray'));
  byId('quit').addEventListener('click', () => call('quit_app'));
  byId('refresh').addEventListener('click', () => call('refresh'));
  byId('clear-logs').addEventListener('click', async () => {
    await invoke('clear_logs');
    await loadState(false);
  });

  byId('theme').addEventListener('click', () => {
    const dark = localStorage.getItem('slimblade-theme') !== 'dark';
    localStorage.setItem('slimblade-theme', dark ? 'dark' : 'light');
    document.documentElement.dataset.theme = dark ? 'dark' : 'light';
    byId('theme').querySelector('md-icon').textContent = dark ? '☀' : '☾';
  });

  const zoomStep = (delta) => {
    const current = Number(localStorage.getItem('slimblade-zoom') ?? 100);
    const next = Math.max(60, Math.min(160, current + delta));
    localStorage.setItem('slimblade-zoom', String(next));
    document.documentElement.style.zoom = String(next / 100);
    refreshDynamic();
  };
  byId('zoom-in').addEventListener('click', () => zoomStep(10));
  byId('zoom-out').addEventListener('click', () => zoomStep(-10));

  const natural = byId('natural');
  natural.addEventListener('change', () => {
    state.config.scroll.natural = natural.checked;
    markDirty();
  });

  const speed = byId('speed');
  speed.addEventListener('input', () => {
    state.config.scroll.speed = Number(speed.value);
    byId('speed-value').textContent = `${Number(speed.value).toFixed(1)}×`;
    markDirty();
  });
  byId('speed-reset').addEventListener('click', () => {
    state.config.scroll.speed = 1.0;
    speed.value = '1';
    byId('speed-value').textContent = '1.0×';
    markDirty();
  });

  document.querySelectorAll('[data-device]').forEach((field) => {
    field.addEventListener('change', () => {
      const key = field.dataset.device;
      const raw = field.value.trim();
      if (key === 'vendor' || key === 'product') {
        const parsed = Number.parseInt(raw.replace(/^0x/i, ''), 16);
        state.config.device[key] = Number.isFinite(parsed) && raw ? parsed : null;
      } else {
        state.config.device[key] = raw || null;
      }
      markDirty();
    });
  });

  byId('device-default').addEventListener('click', () => {
    Object.assign(state.config.device, {
      vendor: 0x047d,
      product: null,
      name: 'Slimblade',
      path: null,
    });
    dirty = true;
    render();
  });

  bindTrackball();

  const dlgClose = document.getElementById('dlg-close');
  if (dlgClose) {
    dlgClose.addEventListener('click', () => {
      document.getElementById('button-dialog').close();
    });
  }
}

function bindTrackball() {
  document.querySelectorAll('.tb-zone').forEach((zone) => {
    zone.addEventListener('click', () => openButtonDialog(zone.dataset.button));
  });
}

function openButtonDialog(name) {
  selectedButton = name;
  const diagram = document.getElementById('trackball');
  if (diagram) {
    diagram.innerHTML = trackballSvg();
    bindTrackball();
  }
  const dialog = document.getElementById('button-dialog');
  if (!dialog) return;

  const form = actionToForm(state.config.buttons[name]);
  dialog.querySelector('#dlg-title').textContent = buttonLabel(name);
  const kind = dialog.querySelector('#dlg-kind');
  const value = dialog.querySelector('#dlg-value');
  const hint = dialog.querySelector('#dlg-hint');

  kind.value = form.kind;
  value.value = form.value;
  value.disabled = form.kind === 'none';
  hint.textContent = {
    mouse: 'Кнопка мыши: left, right, middle, side, extra.',
    drag_lock: 'Кнопка, удерживаемая при drag lock.',
    keys: 'Клавиши через запятую, например ctrl, shift, t.',
    exec: 'Команда running через /bin/sh -c.',
    none: 'Кнопка игнорируется.',
  }[form.kind];

  const applyField = () => {
    state.config.buttons[name] = formToAction(kind.value, value.value);
    value.disabled = kind.value === 'none';
    hint.textContent = {
      mouse: 'Кнопка мыши: left, right, middle, side, extra.',
      drag_lock: 'Кнопка, удерживаемая при drag lock.',
      keys: 'Клавиши через запятую, например ctrl, shift, t.',
      exec: 'Команда running через /bin/sh -c.',
      none: 'Кнопка игнорируется.',
    }[kind.value];
    markDirty();
    document.getElementById('trackball').innerHTML = trackballSvg();
    bindTrackball();
  };
  kind.onchange = applyField;
  value.onchange = applyField;

  dialog.show();
}

async function call(command, payload) {
  try {
    const result = payload === undefined ? await invoke(command) : await invoke(command, payload);
    notice = { kind: 'info', text: String(result ?? 'Готово') };
    dirty = false;
    await loadState(true);
  } catch (err) {
    notice = { kind: 'error', text: String(err) };
    render();
  }
}

async function loadState(full = false) {
  try {
    const fresh = await invoke('get_state');
    const wasDirty = dirty;
    const localConfig = !full && state ? state.config : null;
    state = fresh;
    if (localConfig) state.config = localConfig;
    dirty = wasDirty;
    if (full) render();
    else refreshDynamic();
  } catch (err) {
    notice = { kind: 'error', text: `Не удалось получить состояние: ${err}` };
    if (!state) {
      state = {
        config_path: '',
        config: { device: {}, scroll: { natural: false, speed: 1 }, buttons: {} },
        statuses: {},
        logs: [],
      };
    }
    render();
  }
}

loadState(true).then(() => {
  setInterval(() => loadState(false), 1000);
});
