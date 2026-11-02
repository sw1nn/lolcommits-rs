// Gallery behaviour shared by every style. Style fragments under /ui/styles/
// are markup only; this file binds commit data into them (see the data-*
// attributes below).

// Keep in sync with KNOWN_STYLES in index.html.
const STYLES = [
    { id: 'plain', name: 'Plain', fixed: false },
    { id: 'museum', name: 'Gallery wall', fixed: true },
    { id: 'news', name: 'Newsroom', fixed: true },
    { id: 'film', name: 'Darkroom', fixed: true },
    { id: 'booth', name: 'Photo booth', fixed: true },
    { id: 'term', name: 'Terminal', fixed: true },
];
const DEFAULT_STYLE = 'plain';
const STORAGE_KEY = 'lolcommits.style';
const STRIP_SIZE = 5;
const TICKER_SIZE = 10;

const state = {
    images: [],
    index: 0,
    galleryTitle: 'Lolcommits Gallery',
    newImages: 0,
    infoOn: false,
    activeStyle: null,
};

const preloaded = new Map();
let switchToken = 0;

const stage = () => document.getElementById('stage');

function formatStatNumber(n) {
    if (n <= 999) return n.toString();
    if (n < 1000000) return (n / 1000).toFixed(1) + 'k';
    return (n / 1000000).toFixed(1) + 'M';
}

function parseSubject(message) {
    const subject = (message || '').split('\n')[0];
    const colon = subject.indexOf(':');
    const stripped = colon >= 0 ? subject.slice(colon + 1).trim() : subject;
    const capitalised = stripped.charAt(0).toUpperCase() + stripped.slice(1);
    const title = capitalised.length > 70 ? capitalised.slice(0, 67) + '...' : capitalised;
    return { subject, title };
}

const pad = n => String(n).padStart(2, '0');

function dateFields(timestamp) {
    const date = new Date(timestamp);
    if (!timestamp || Number.isNaN(date.getTime())) {
        return { day: timestamp || '', time: '', clock: '', shortdate: '', gitdate: '', year: '' };
    }
    const offset = -date.getTimezoneOffset();
    const tz = (offset >= 0 ? '+' : '-') + pad(Math.floor(Math.abs(offset) / 60)) + pad(Math.abs(offset) % 60);
    const weekday = date.toLocaleDateString('en-US', { weekday: 'short' });
    const month = date.toLocaleDateString('en-US', { month: 'short' });
    const hms = `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
    return {
        day: date.toLocaleDateString('en-US', { weekday: 'long', year: 'numeric', month: 'long', day: 'numeric' }),
        time: date.toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit', hour12: true }),
        clock: `${pad(date.getHours())}:${pad(date.getMinutes())}`,
        shortdate: `${date.getDate()} ${month} ${date.getFullYear()}`,
        gitdate: `${weekday} ${month} ${date.getDate()} ${hms} ${date.getFullYear()} ${tz}`,
        year: String(date.getFullYear()),
    };
}

function commitFields(image, index) {
    const total = state.images.length;
    const { subject, title } = parseSubject(image.message);
    const type = (image.commit_type || 'commit').toUpperCase();
    const scope = image.scope || '';
    const repo = image.repo_name || '';
    const stats = image.stats || {};
    return {
        title,
        subject,
        message: image.message || '',
        hash: (image.revision || '').slice(0, 7),
        type,
        scope,
        typescope: scope ? `${type} · ${scope}` : type,
        infoline: [type, scope, repo].filter(Boolean).join(' • '),
        repo,
        branch: image.branch_name || '',
        files: formatStatNumber(stats.files_changed || 0),
        ins: formatStatNumber(stats.insertions || 0),
        del: formatStatNumber(stats.deletions || 0),
        ...dateFields(image.timestamp),
        n: String(index + 1),
        pos: String(total - index),
        total: String(total),
        gallery: state.galleryTitle,
    };
}

function imageUrl(image) {
    return `/images/${encodeURIComponent(image.filename)}`;
}

// With skipStrips, elements inside a [data-strip] are left alone: they belong
// to their frame's commit, not to the current one.
function bindCommit(root, image, index, skipStrips) {
    const fields = commitFields(image, index);
    const inScope = el => !skipStrips || !el.closest('[data-strip]');
    root.querySelectorAll('[data-f]').forEach(el => {
        if (!inScope(el)) return;
        const name = el.dataset.f;
        el.textContent = fields[name] ?? '';
        if (name === 'title' || name === 'message') el.title = fields.message;
    });
    // data-hide-zero on a wrapper hides it when its bound value is "0"
    root.querySelectorAll('[data-hide-zero]').forEach(el => {
        if (!inScope(el)) return;
        el.hidden = el.querySelector('[data-f]')?.textContent === '0';
    });
    root.querySelectorAll('img[data-photo]').forEach(img => {
        if (!inScope(img)) return;
        img.src = imageUrl(image);
        img.alt = `Lolcommit ${fields.hash}`;
    });
}

// Indices for a strip of `size` frames around `index`, oldest first. The
// window clamps at both ends so the strip stays full when it can.
function stripWindow(index, total, size) {
    const count = Math.min(size, total);
    const newest = Math.min(Math.max(index - Math.floor((count - 1) / 2), 0), total - count);
    const indices = [];
    for (let i = newest + count - 1; i >= newest; i--) indices.push(i);
    return indices;
}

function renderStrips(root) {
    root.querySelectorAll('[data-strip]').forEach(strip => {
        const template = strip.querySelector('template[data-strip-frame]');
        if (!template) return;
        // Rebuilding the frames would drop keyboard focus from the focused one
        const focused = document.activeElement;
        const focusedItem = focused?.closest?.('[data-strip-item]');
        const refocus = focusedItem && strip.contains(focusedItem) ? focusedItem.dataset.stripItem : null;
        strip.querySelectorAll('[data-strip-item]').forEach(el => el.remove());
        for (const i of stripWindow(state.index, state.images.length, STRIP_SIZE)) {
            const frame = template.content.firstElementChild.cloneNode(true);
            frame.dataset.stripItem = String(i);
            frame.classList.toggle('is-current', i === state.index);
            bindCommit(frame, state.images[i], i, false);
            strip.appendChild(frame);
        }
        if (refocus !== null) {
            const frames = [...strip.querySelectorAll('[data-strip-item]')];
            const target = frames.find(f => f.dataset.stripItem === refocus) ?? frames.find(f => f.classList.contains('is-current'));
            target?.focus();
        }
    });
}

function renderTicker(root) {
    root.querySelectorAll('[data-ticker]').forEach(ticker => {
        const items = state.images.slice(0, TICKER_SIZE).map((image, i) => {
            const fields = commitFields(image, i);
            const span = document.createElement('span');
            span.textContent = `${fields.title} — ${fields.hash}`;
            return span;
        });
        // A second copy makes the scrolling loop seamless
        ticker.replaceChildren(...items, ...items.map(span => span.cloneNode(true)));
    });
}

function renderCounter() {
    document.getElementById('counterText').textContent = `${state.index + 1} / ${state.images.length}`;
    document.getElementById('newImagesBadge').hidden = state.newImages === 0;
}

function preloadAround(index) {
    for (const i of [index - 1, index + 1]) {
        const image = state.images[i];
        if (image && !preloaded.has(image.filename)) {
            const img = new Image();
            img.src = imageUrl(image);
            preloaded.set(image.filename, img);
        }
    }
}

function render() {
    const root = stage();
    const image = state.images[state.index];
    if (!image || state.activeStyle === null) return;
    bindCommit(root, image, state.index, true);
    root.querySelectorAll('[data-nav="older"]').forEach(b => { b.disabled = state.index >= state.images.length - 1; });
    root.querySelectorAll('[data-nav="newer"]').forEach(b => { b.disabled = state.index <= 0; });
    renderStrips(root);
    renderTicker(root);
    renderCounter();
    preloadAround(state.index);
}

function show(index) {
    if (index < 0 || index >= state.images.length) return;
    state.index = index;
    state.newImages = 0;
    render();
}

// images are sorted newest first, so a higher index is older
const older = () => show(state.index + 1);
const newer = () => show(state.index - 1);

function toggleInfo() {
    state.infoOn = !state.infoOn;
    stage().classList.toggle('info-on', state.infoOn);
    document.getElementById('infoButton').setAttribute('aria-pressed', String(state.infoOn));
}

function savedStyle() {
    try {
        const id = localStorage.getItem(STORAGE_KEY);
        return STYLES.some(s => s.id === id) ? id : DEFAULT_STYLE;
    } catch {
        return DEFAULT_STYLE;
    }
}

function saveStyle(id) {
    try {
        localStorage.setItem(STORAGE_KEY, id);
    } catch (error) {
        console.error('Could not save the gallery style', error);
    }
}

async function fetchFragment(id) {
    const response = await fetch(`/ui/styles/${id}.html`);
    if (!response.ok) throw new Error(`GET /ui/styles/${id}.html returned ${response.status}`);
    return response.text();
}

function loadStylesheet(id) {
    const link = document.createElement('link');
    link.rel = 'stylesheet';
    link.href = `/ui/styles/${id}.css`;
    link.dataset.style = id;
    // Held back until applyStyle swaps the markup, so the old markup is never
    // restyled by the new sheet
    link.media = 'print';
    const ready = new Promise((resolve, reject) => {
        link.onload = resolve;
        link.onerror = () => reject(new Error(`Could not load ${link.href}`));
    });
    document.head.appendChild(link);
    return { link, ready };
}

// Web fonts are optional: each fixed style names system fallbacks, so the font
// sheet is separate from the style sheet and never awaited. A failure here
// must not stop the style from applying.
function applyFonts(style) {
    const existing = document.getElementById('style-fonts');
    if (!style.fixed) {
        existing?.remove();
        return;
    }
    const href = `/ui/styles/${style.id}.fonts.css`;
    if (existing && existing.getAttribute('href') === href) return;
    existing?.remove();
    const link = document.createElement('link');
    link.rel = 'stylesheet';
    link.id = 'style-fonts';
    link.href = href;
    link.onerror = () => console.warn('Could not load the web fonts, using fallbacks', href);
    document.head.appendChild(link);
}

// Resolves true when the style is active. On failure the previous style stays.
async function applyStyle(id) {
    const style = STYLES.find(s => s.id === id) ?? STYLES[0];
    const token = ++switchToken;
    const currentLink = document.getElementById('style-css');
    const reuse = currentLink && currentLink.dataset.style === style.id;
    const { link, ready } = reuse ? { link: currentLink, ready: Promise.resolve() } : loadStylesheet(style.id);
    try {
        const [html] = await Promise.all([fetchFragment(style.id), ready]);
        if (token !== switchToken) {
            if (!reuse) link.remove();
            return false;
        }
        if (!reuse) {
            currentLink?.remove();
            link.id = 'style-css';
        }
        const root = stage();
        link.media = 'all';
        // Fragments are our own static files, never commit data
        root.innerHTML = html;
        root.dataset.style = style.id;
        root.classList.toggle('fixed', style.fixed);
        state.activeStyle = style.id;
        hidePickerError();
        render();
        renderPicker();
        renderInfoButton();
        applyFonts(style);
        return true;
    } catch (error) {
        if (!reuse) link.remove();
        if (token !== switchToken) return false;
        console.error('Could not load the gallery style', style.id, error);
        showPickerError(`Could not load ${style.name}`);
        return false;
    }
}

function pickerItems() {
    return [...document.querySelectorAll('#pickerMenu [role="menuitemradio"]')];
}

function renderPicker() {
    pickerItems().forEach(item => item.setAttribute('aria-checked', String(item.dataset.style === state.activeStyle)));
}

// Only the Plain style hides its metadata behind the toggle; a fixed style
// shows it always, so the control is hidden there.
function renderInfoButton() {
    const style = STYLES.find(s => s.id === state.activeStyle);
    const button = document.getElementById('infoButton');
    button.hidden = style?.fixed !== false;
    button.setAttribute('aria-pressed', String(state.infoOn));
}

function showPickerError(message) {
    const el = document.getElementById('pickerError');
    el.textContent = message;
    el.hidden = false;
}

function hidePickerError() {
    document.getElementById('pickerError').hidden = true;
}

function openPicker() {
    document.getElementById('pickerMenu').hidden = false;
    document.getElementById('pickerButton').setAttribute('aria-expanded', 'true');
    (pickerItems().find(i => i.getAttribute('aria-checked') === 'true') ?? pickerItems()[0])?.focus();
}

function closePicker(returnFocus) {
    const menu = document.getElementById('pickerMenu');
    if (menu.hidden) return;
    menu.hidden = true;
    document.getElementById('pickerButton').setAttribute('aria-expanded', 'false');
    if (returnFocus) document.getElementById('pickerButton').focus();
}

async function chooseStyle(id) {
    if (await applyStyle(id)) {
        saveStyle(id);
        closePicker(true);
    }
}

function setupPicker() {
    const menu = document.getElementById('pickerMenu');
    for (const style of STYLES) {
        const item = document.createElement('button');
        item.type = 'button';
        item.setAttribute('role', 'menuitemradio');
        item.setAttribute('aria-checked', 'false');
        item.dataset.style = style.id;
        item.textContent = style.name;
        item.addEventListener('click', () => chooseStyle(style.id));
        menu.insertBefore(item, document.getElementById('pickerError'));
    }
    document.getElementById('pickerButton').addEventListener('click', () => {
        if (menu.hidden) openPicker();
        else closePicker(true);
    });
    menu.addEventListener('keydown', e => {
        if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
        e.preventDefault();
        const items = pickerItems();
        const at = items.indexOf(document.activeElement);
        const next = (at + (e.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length;
        items[next].focus();
    });
    document.addEventListener('click', e => {
        if (!document.getElementById('picker').contains(e.target)) closePicker(false);
    });
}

function setupInfoButton() {
    document.getElementById('infoButton').addEventListener('click', toggleInfo);
}

// Every style routes its navigation through [data-nav], so these bindings work
// whatever style is active. Each style's markup names them in its nav controls.
function setupKeys() {
    document.addEventListener('keydown', e => {
        if (e.altKey || e.ctrlKey || e.metaKey) return;
        if (e.key === 'Escape') {
            closePicker(true);
            return;
        }
        // The open menu handles its own keys
        if (e.target instanceof Element && e.target.closest('#pickerMenu')) return;
        // Caps Lock and Shift make e.key an upper-case letter; the bindings are
        // case-insensitive
        const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
        switch (key) {
            case 'ArrowLeft':
            case 'h':
                e.preventDefault();
                older();
                break;
            case 'ArrowRight':
            case 'l':
                e.preventDefault();
                newer();
                break;
            case ' ':
                // Space presses the picker's buttons; a clicked nav button keeps
                // focus, so exempting every button would break the toggle
                if (e.target instanceof Element && e.target.closest('#pill')) return;
                e.preventDefault();
                toggleInfo();
                break;
        }
    });
}

function setupStage() {
    stage().addEventListener('click', e => {
        if (!(e.target instanceof Element)) return;
        const nav = e.target.closest('[data-nav]');
        if (nav && !nav.disabled) {
            if (nav.dataset.nav === 'older') older();
            else newer();
            return;
        }
        const frame = e.target.closest('[data-strip-item]');
        if (frame) show(Number(frame.dataset.stripItem));
    });
}

async function loadConfig() {
    try {
        const response = await fetch('/api/config');
        if (!response.ok) throw new Error(`GET /api/config returned ${response.status}`);
        const config = await response.json();
        state.galleryTitle = config.gallery_title;
        document.title = config.gallery_title;
        render();
    } catch (error) {
        // The default title stays
        console.error('Could not load the gallery config', error);
    }
}

// Keeps the displayed commit when the list changes under it.
async function loadImages() {
    const response = await fetch('/api/images');
    if (!response.ok) throw new Error(`GET /api/images returned ${response.status}`);
    const images = await response.json();
    // Read after the await: a navigation during the fetch must not be undone
    const shown = state.images[state.index]?.revision;
    state.images = images;
    const found = shown ? state.images.findIndex(image => image.revision === shown) : -1;
    state.index = found >= 0 ? found : Math.min(state.index, Math.max(state.images.length - 1, 0));
}

function hideLoading() {
    document.getElementById('loading').classList.add('hidden');
}

function showError() {
    hideLoading();
    stage().hidden = true;
    document.getElementById('pill').hidden = true;
    document.getElementById('error').hidden = false;
}

async function showGallery() {
    document.getElementById('error').hidden = true;
    stage().hidden = false;
    document.getElementById('pill').hidden = false;
    if (!(await applyStyle(savedStyle())) && state.activeStyle === null) {
        await applyStyle(DEFAULT_STYLE);
    }
    hideLoading();
}

function setupSSE() {
    const events = new EventSource('/api/events');
    events.onmessage = async event => {
        if (event.data !== 'new_image') return;
        state.newImages++;
        try {
            await loadImages();
        } catch (error) {
            console.error('Could not refresh the image list', error);
            return;
        }
        if (state.activeStyle === null && state.images.length > 0) await showGallery();
        else render();
    };
    // EventSource reconnects by itself
    events.onerror = error => console.error('SSE connection error', error);
}

async function boot() {
    setupKeys();
    setupStage();
    setupPicker();
    setupInfoButton();
    loadConfig();
    try {
        await loadImages();
    } catch (error) {
        console.error('Could not load the image list', error);
        showError();
        setupSSE();
        return;
    }
    if (state.images.length === 0) showError();
    else await showGallery();
    setupSSE();
}

boot();
