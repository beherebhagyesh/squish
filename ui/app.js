const { invoke } = window.__TAURI__.core;
const { open: openDialog } = window.__TAURI__.dialog;

// Guard: if Tauri APIs aren't available, surface a clear error
if (!window.__TAURI__) {
  document.body.innerHTML = '<p style="padding:2rem;color:red">Tauri API not found. Make sure withGlobalTauri is enabled.</p>';
  throw new Error('Tauri not available');
}

// --- DOM refs ---
const sourceInput       = document.getElementById('sourcePath');
const outputBaseInput   = document.getElementById('outputBasePath');
const outputFolderInput = document.getElementById('outputFolderName');
const scanBtn           = document.getElementById('scanBtn');
const convertBtn        = document.getElementById('convertBtn');
const resultsPanel      = document.getElementById('resultsPanel');
const logPanel          = document.getElementById('logPanel');
const resultsBody       = document.getElementById('resultsBody');
const summaryText       = document.getElementById('summaryText');
const resultsTitle      = document.getElementById('resultsTitle');
const statsGrid         = document.getElementById('statsGrid');
const logList           = document.getElementById('logList');
const modeGroup         = document.getElementById('modeGroup');
const presetGroup       = document.getElementById('presetGroup');
const outputFormatField = document.getElementById('outputFormatField');
const outputFormatSel   = document.getElementById('outputFormat');
const qualityRange      = document.getElementById('qualityRange');
const qualityInput      = document.getElementById('qualityInput');
const maxWidthInput     = document.getElementById('maxWidth');
const maxHeightInput    = document.getElementById('maxHeight');
const targetKbInput     = document.getElementById('targetKb');
const stripMetaChk      = document.getElementById('stripMeta');
const openOutputBtn     = document.getElementById('openOutputBtn');

// --- State ---
let currentImages = [];
let outputRoot    = '';
let scanDone      = false;
let activeMode    = 'keep_format';
let activePreset  = 'balanced';

const PRESET_DEFAULTS = {
  lossless:   { quality: 100, max_width: null },
  balanced:   { quality: 82,  max_width: 2000 },
  small_file: { quality: 68,  max_width: 1600 },
};

const MODES_WITH_FORMAT = new Set(['convert_compress']);

// --- Helpers ---
function previewOutputExt(mode, sourceExt, outputFormat) {
  if (mode === 'keep_format') return sourceExt === '.jpeg' ? '.jpg' : sourceExt;
  if (mode === 'to_webp_lossy' || mode === 'to_webp_lossless') return '.webp';
  const fmt = (outputFormat || 'webp').toLowerCase();
  return '.' + (fmt === 'jpg' || fmt === 'jpeg' ? 'jpg' : fmt);
}

function fmt(label) { return label.replace(/^\./, '').toUpperCase(); }
function kb(bytes)  { return bytes ? (bytes / 1024).toFixed(1) + ' KB' : '—'; }
function pct(val) {
  if (val > 0)  return `<span class="saved-positive">▼ ${val}%</span>`;
  if (val < 0)  return `<span class="saved-negative">▲ ${Math.abs(val)}%</span>`;
  return '0%';
}
const basename = s => (s || '').replace(/\\/g, '/').split('/').pop();

function setButtonState(button, busy, busyLabel) {
  button.disabled = busy;
  if (busy) button.dataset.idleLabel = button.textContent;
  button.textContent = busy ? busyLabel : (button.dataset.idleLabel || button.textContent);
}

function addLog(message, isError = false) {
  logPanel.classList.remove('hidden');
  const li = document.createElement('li');
  li.textContent = message;
  if (isError) li.classList.add('error');
  logList.appendChild(li);
}

// --- Mode / Preset UI ---
function updateModeUI() {
  document.querySelectorAll('.mode-btn').forEach(btn =>
    btn.classList.toggle('active', btn.dataset.mode === activeMode));
  outputFormatField.style.display = MODES_WITH_FORMAT.has(activeMode) ? '' : 'none';
  updateConvertBtnLabel();
  if (scanDone) refreshPreviewTable();
}

function updatePresetUI() {
  document.querySelectorAll('.preset-btn').forEach(btn =>
    btn.classList.toggle('active', btn.dataset.preset === activePreset));
  const p = PRESET_DEFAULTS[activePreset];
  if (p) {
    qualityRange.value  = p.quality;
    qualityInput.value  = p.quality;
    maxWidthInput.value = p.max_width || '';
  }
}

function updateConvertBtnLabel() {
  const labels = {
    keep_format:      'Compress Images',
    to_webp_lossy:    'Convert to WebP',
    to_webp_lossless: 'Convert to WebP (Lossless)',
    convert_compress: 'Convert & Compress',
  };
  convertBtn.textContent = labels[activeMode] || 'Convert';
  convertBtn.dataset.idleLabel = convertBtn.textContent;
}

modeGroup.addEventListener('click', e => {
  const btn = e.target.closest('.mode-btn');
  if (!btn) return;
  activeMode = btn.dataset.mode;
  updateModeUI();
});

presetGroup.addEventListener('click', e => {
  const btn = e.target.closest('.preset-btn');
  if (!btn) return;
  activePreset = btn.dataset.preset;
  updatePresetUI();
});

qualityRange.addEventListener('input', () => { qualityInput.value = qualityRange.value; });
qualityInput.addEventListener('input', () => {
  const v = Math.min(100, Math.max(1, parseInt(qualityInput.value) || 82));
  qualityRange.value = v;
  qualityInput.value = v;
});
outputFormatSel.addEventListener('change', () => { if (scanDone) refreshPreviewTable(); });

// --- Native folder pickers ---
document.getElementById('pickSourceBtn').addEventListener('click', async () => {
  const selected = await openDialog({ directory: true, multiple: false, title: 'Choose source folder' });
  if (selected) sourceInput.value = selected;
});

document.getElementById('pickOutputBtn').addEventListener('click', async () => {
  const selected = await openDialog({ directory: true, multiple: false, title: 'Choose output base folder' });
  if (selected) outputBaseInput.value = selected;
});

// --- Stats ---
function renderStats(counts = {}) {
  statsGrid.classList.remove('hidden');
  statsGrid.innerHTML = `
    <article class="stat-card">
      <span>Images found</span>
      <strong>${counts.total_images ?? 0}</strong>
    </article>`;
}

// --- Preview table ---
function refreshPreviewTable() {
  resultsBody.innerHTML = currentImages.map(item => {
    const outExt = previewOutputExt(activeMode, item.source_ext, outputFormatSel.value);
    return `<tr>
      <td><code>${item.relative}</code></td>
      <td><span class="badge">${fmt(item.source_ext)}</span></td>
      <td><span class="badge badge-out">${fmt(outExt)}</span></td>
      <td class="hidden"></td><td class="hidden"></td>
      <td class="hidden"></td><td class="hidden"></td>
    </tr>`;
  }).join('');
}

// --- Post-convert table ---
function showPostConvertTable(results) {
  ['colSizeIn','colSizeOut','colReduction','colStatus'].forEach(id =>
    document.getElementById(id).classList.remove('hidden'));

  resultsBody.innerHTML = results.map(r => {
    const statusCell = r.status === 'ok'
      ? `<span class="status-ok">✓</span>`
      : `<span class="status-err" title="${r.error || ''}">✗ ${r.error || 'Error'}</span>`;
    const warnings = (r.warnings || []).map(w =>
      `<span class="warning-tag" title="${w}">⚠</span>`).join(' ');
    return `<tr class="${r.status !== 'ok' ? 'row-error' : ''}">
      <td><code>${basename(r.source)}</code>${warnings ? ' ' + warnings : ''}</td>
      <td><span class="badge">${fmt(r.source_ext)}</span></td>
      <td><span class="badge badge-out">${fmt(r.output_ext)}</span></td>
      <td>${kb(r.bytes_in)}</td>
      <td>${kb(r.bytes_out)}</td>
      <td>${pct(r.reduction_pct)}</td>
      <td>${statusCell}</td>
    </tr>`;
  }).join('');
}

// --- Scan ---
scanBtn.addEventListener('click', async () => {
  const source_path        = sourceInput.value.trim();
  const output_base_path   = outputBaseInput.value.trim();
  const output_folder_name = outputFolderInput.value.trim();

  if (!source_path || !output_base_path || !output_folder_name) {
    addLog('Fill in source path, output base path, and output folder name first.', true);
    return;
  }

  try {
    setButtonState(scanBtn, true, 'Scanning…');
    resultsPanel.classList.add('hidden');
    statsGrid.classList.add('hidden');
    ['colSizeIn','colSizeOut','colReduction','colStatus'].forEach(id =>
      document.getElementById(id).classList.add('hidden'));

    const data = await invoke('scan', { req: { source_path, output_base_path, output_folder_name } });
    currentImages = data.images || [];
    outputRoot    = data.output_root;
    scanDone      = true;

    renderStats(data.counts);
    summaryText.textContent  = `${currentImages.length} image(s) found in ${data.working_folder}`;
    resultsTitle.textContent = 'Discovered Images — Preview';
    refreshPreviewTable();
    resultsPanel.classList.remove('hidden');
    convertBtn.disabled = currentImages.length === 0;
    openOutputBtn.classList.add('hidden');
    addLog(`Scan complete: ${currentImages.length} image(s). Output → ${data.output_root}`);
  } catch (err) {
    addLog(`Scan failed: ${err}`, true);
    renderStats();
  } finally {
    setButtonState(scanBtn, false);
  }
});

// --- Convert ---
convertBtn.addEventListener('click', async () => {
  if (!currentImages.length) {
    addLog('No images to convert. Scan first.', true);
    return;
  }

  try {
    setButtonState(convertBtn, true, 'Converting…');
    setButtonState(scanBtn, true, 'Please wait…');

    const data = await invoke('convert', {
      req: {
        images:         currentImages,
        output_root:    outputRoot,
        mode:           activeMode,
        preset:         activePreset,
        output_format:  outputFormatSel.value || null,
        quality:        parseInt(qualityInput.value) || null,
        lossless:       null,
        max_width:      parseInt(maxWidthInput.value)  || null,
        max_height:     parseInt(maxHeightInput.value) || null,
        target_kb:      parseInt(targetKbInput.value)  || null,
        strip_metadata: stripMetaChk.checked,
      }
    });

    resultsTitle.textContent = 'Conversion Results';
    showPostConvertTable(data.results);

    const ok       = data.results.filter(r => r.status === 'ok').length;
    const err      = data.results.length - ok;
    const totalIn  = data.results.reduce((s, r) => s + r.bytes_in, 0);
    const totalOut = data.results.reduce((s, r) => s + r.bytes_out, 0);
    const overallPct = totalIn ? ((1 - totalOut / totalIn) * 100).toFixed(1) : 0;

    summaryText.textContent = `${ok} converted${err ? `, ${err} failed` : ''} — ${kb(totalIn)} → ${kb(totalOut)} (${overallPct}% reduction)`;
    addLog(`Done. ${ok} converted, ${err} failed. Overall reduction: ${overallPct}%.`);
    data.results.forEach(r =>
      (r.warnings || []).forEach(w => addLog(`⚠ ${basename(r.source)}: ${w}`)));

    openOutputBtn.classList.remove('hidden');
  } catch (err) {
    addLog(`Conversion failed: ${err}`, true);
  } finally {
    setButtonState(convertBtn, false);
    setButtonState(scanBtn, false);
  }
});

// --- Open output folder ---
openOutputBtn.addEventListener('click', async () => {
  if (outputRoot) await invoke('open_folder', { path: outputRoot });
});

// --- Init ---
updateModeUI();
updatePresetUI();
