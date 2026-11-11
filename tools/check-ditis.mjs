// Check this crate against DiTiS's own code: STS and DiTiS's JSON file.
//
//   node tools/check-ditis.mjs /path/to/ditis
//
// DiTiS (https://github.com/MisakiAkatsuki/ditis) is a browser app, but its
// readers and writers are plain functions in js/parsers.js and
// js/fileOperations.js. This script takes those functions out of a DiTiS
// checkout and runs them under Node, so every file below is one DiTiS itself
// wrote or read — nothing is a reimplementation of DiTiS. The crate's side
// is `cargo run --example convert`. Set CARGO_ARGS to pass more to cargo
// (`--offline`, say).
//
// Exit status 0 when every check passes.

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const ditis = process.argv[2];
if (!ditis) {
    console.error('usage: node tools/check-ditis.mjs /path/to/ditis');
    process.exit(2);
}
const repo = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'xdts-ditis-'));
const at = (name) => path.join(tmp, name);

// --- DiTiS's own functions ---------------------------------------------------

const source = (f) => fs.readFileSync(path.join(ditis, 'js', f), 'utf8');
const parsers = source('parsers.js');
const fileOps = source('fileOperations.js');

/** The text of `function name(...) {...}` in `src`. */
function grab(src, name) {
    const start = src.indexOf(`function ${name}(`);
    if (start < 0) throw new Error(`DiTiS has no function ${name}`);
    let depth = 0;
    for (let i = src.indexOf('{', src.indexOf(')', start)); i < src.length; i++) {
        if (src[i] === '{') depth++;
        else if (src[i] === '}' && --depth === 0) return src.slice(start, i + 1);
    }
    throw new Error(`function ${name} never ends`);
}
const define = (src, name, edit = (s) => s) =>
    (0, eval)(`globalThis.${name} = ${edit(grab(src, name)).replace(`function ${name}`, 'function')}`);

// What the functions reach for in the app, and nothing more.
Object.assign(globalThis, {
    AppState: { fps: 24, sheets: [] },
    ErrorLevel: {},
    showErrorToast() {}, debugLog() {}, saveHistory() {},
    renderTabs() {}, renderSpreadsheet() {}, updateWindowTitle() {},
    getLayerName: (i) => String.fromCharCode(65 + i),
    TIMESHEET_SYMBOLS: Object.freeze({ SYMBOL_NULL_CELL: '×', SYMBOL_TICK_1: '○', SYMBOL_TICK_2: '●' }),
});
for (const name of ['encodeTimesheetValue', 'cloneTimesheetData', 'buildTimesheetTable',
                    'generateXdtsData', 'importExchangeTimesheet', 'generateStsBuffer']) {
    define(parsers, name);
}
// parseStsFile adds the sheet to the app; return it instead.
define(parsers, 'parseStsFile', (s) => s.slice(0, s.lastIndexOf('AppState.sheets.push')) + 'return newSheet;}');
for (const name of ['normalizeSheetForSave', 'buildDitisJson', 'loadSheetData']) define(fileOps, name);

// --- Helpers -----------------------------------------------------------------

function convert(input, ...outputs) {
    const extra = (process.env.CARGO_ARGS || '').split(/\s+/).filter(Boolean);
    execFileSync('cargo', ['run', '--quiet', ...extra, '--example', 'convert', '--', input, ...outputs],
        { cwd: repo, stdio: ['ignore', 'inherit', 'inherit'] });
}

/** A DiTiS sheet as the cells each frame shows, the way DiTiS's XDTS export
 * reads them: × and "" are both empty, and - continues the frame before. */
function view(sheet) {
    return sheet.layers.map((layer) => {
        const out = [];
        let last = 'SYMBOL_NULL_CELL';
        for (let f = 1; f <= sheet.frames; f++) {
            const v = encodeTimesheetValue(sheet.data[f]?.[layer.id]);
            if (v !== 'SYMBOL_HYPHEN') last = v;
            out.push(last);
        }
        return out;
    }).filter((col) => col.some((v) => v !== 'SYMBOL_NULL_CELL'));
}

/** Open a .ditis as DiTiS does. */
function loadDitis(file) {
    const sheets = JSON.parse(fs.readFileSync(file, 'utf8')).sheets;
    sheets.forEach(loadSheetData);
    return sheets;
}

/** Import an .xdts as DiTiS does. */
function importXdts(file) {
    AppState.sheets = [];
    importExchangeTimesheet(fs.readFileSync(file, 'utf8'), path.basename(file), 'XDTS');
    return AppState.sheets;
}

/** A sheet as DiTiS's createNewSheet makes one, then edited. */
function newSheet() {
    const s = { name: 'Sheet1', fps: 24, layers: [], frames: 12, framePageSize: 144, data: {},
                insertedFrames: [2], disabledFrames: [4] };
    for (let i = 0; i < 26; i++) s.layers.push({ id: `L${i + 1}`, name: getLayerName(i) });
    s.layers[1].name = '背景';
    const put = (f, l, v) => { (s.data[f] ||= {})[l] = String(v); };
    [1, 1, 1, 2, 2, '×', '○', '○', 3, 3].forEach((v, i) => put(i + 1, 'L1', v));
    for (let f = 1; f <= 12; f++) put(f, 'L2', 5);
    return s;
}

const results = [];
function check(name, fn) {
    try {
        const why = fn();
        results.push([why ? 'FAIL' : 'ok', name, why || '']);
    } catch (e) {
        results.push(['FAIL', name, e.message]);
    }
}
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const bytes = (f) => fs.readFileSync(f);

// --- STS ---------------------------------------------------------------------

check('STS: a file DiTiS writes comes back byte for byte', () => {
    fs.writeFileSync(at('a.sts'), Buffer.from(generateStsBuffer(stsSheet())));
    convert(at('a.sts'), at('b.sts'));
    if (!bytes(at('a.sts')).equals(bytes(at('b.sts')))) return 'the bytes differ';
});
check('STS: DiTiS reads what this crate writes as the sheet it wrote', () => {
    const back = parseStsFile(new Uint8Array(bytes(at('b.sts'))), 'b.sts');
    if (!same(view(back), view(stsSheet()))) return 'the cells differ';
});

/** STS holds numbers only: the new sheet without its marks. */
function stsSheet() {
    const s = newSheet();
    s.layers = s.layers.slice(0, 2);
    for (const row of Object.values(s.data)) {
        for (const k of Object.keys(row)) if (!/^\d+$/.test(row[k])) row[k] = '';
    }
    return s;
}

// --- DiTiS's JSON file ---------------------------------------------------------

check('DiTiS: a new sheet comes back byte for byte', () => {
    fs.writeFileSync(at('new.ditis'), buildDitisJson(newSheet()));
    convert(at('new.ditis'), at('new.out.ditis'));
    if (!bytes(at('new.ditis')).equals(bytes(at('new.out.ditis')))) return 'the bytes differ';
});
check('DiTiS: the XDTS this crate writes is the sheet DiTiS wrote', () => {
    convert(at('new.ditis'), at('new.xdts'));
    if (!same(view(importXdts(at('new.xdts'))[0]), view(loadDitis(at('new.ditis'))[0]))) return 'the cells differ';
});

check("DiTiS: the changes this crate reads are the ones DiTiS's own XDTS export writes", () => {
    // Round trips alone would pass a crate that kept ○ as the string "○":
    // it would come back as it went. This compares the instructions.
    const changes = (doc) => Object.fromEntries(doc.timeTables[0].fields
        .find((f) => f.fieldId === 0).tracks
        .map((t) => [t.trackNo, t.frames.map((f) => [f.frame, f.data[0].values[0]])])
        .filter(([, frames]) => frames.length));
    const theirs = changes(generateXdtsData(loadDitis(at('new.ditis'))[0]));
    const xdts = fs.readFileSync(at('new.xdts'), 'utf8');
    const ours = changes(JSON.parse(xdts.slice(xdts.indexOf('\n') + 1)));
    if (!same(ours, theirs)) return `ours ${JSON.stringify(ours)}, DiTiS's ${JSON.stringify(theirs)}`;
});

const csp = path.join(repo, 'tests', 'data', 'csp_v5.xdts');
check('DiTiS: a sheet DiTiS imported from XDTS keeps its cells and its source', () => {
    fs.writeFileSync(at('imported.ditis'), buildDitisJson(importXdts(csp)[0]));
    convert(at('imported.ditis'), at('imported.out.ditis'));
    const [a] = loadDitis(at('imported.ditis'));
    const [b] = loadDitis(at('imported.out.ditis'));
    if (!same(view(a), view(b))) return 'the cells differ';
    if (!same(a.timesheetSource, b.timesheetSource)) return 'timesheetSource differs';
});
check('DiTiS: a .ditis this crate writes from XDTS opens as that sheet', () => {
    convert(csp, at('csp.ditis'));
    if (!same(view(loadDitis(at('csp.ditis'))[0]), view(importXdts(csp)[0]))) return 'the cells differ';
});
check('DiTiS: a .ditis this crate writes from a scene opens', () => {
    convert(path.join(repo, 'tests', 'data', 'synthetic.tnz'), at('scene.ditis'));
    const [s] = loadDitis(at('scene.ditis'));
    if (s.frames !== 10 || view(s).length !== 3) return `${s.frames} frames, ${view(s).length} columns`;
});

// --- Report ------------------------------------------------------------------

for (const [status, name, why] of results) console.log(`${status.padEnd(4)} ${name}${why ? ` — ${why}` : ''}`);
const failed = results.filter(([s]) => s !== 'ok').length;
console.log(`\n${results.length - failed} passed, ${failed} failed (DiTiS at ${ditis})`);
fs.rmSync(tmp, { recursive: true, force: true });
process.exit(failed ? 1 : 0);
