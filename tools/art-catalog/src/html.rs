//! M3: single-page HTML catalog (Chinese UI). Data is embedded as JSON;
//! images use relative paths so the page works from file://.
//!
//! UI focus (2026-08-29 human feedback): keep it simple — four views only:
//! overview+import / assets / gallery / report. The 3D viewer canvas is a
//! PERSISTENT element (never re-created on detail re-render) because
//! re-parenting the canvas breaks the WebGL context binding.

pub fn generate(
    data_json: &str,
    root_prefix: &str,
    generated: &str,
    game_dir: &str,
    library_label: &str,
    legacy_note: &str,
) -> String {
    let mut html = TEMPLATE.to_string();
    html = html.replace("__DATA__", data_json);
    html = html.replace("__ROOT__", root_prefix);
    html = html.replace("__GENERATED__", generated);
    html = html.replace("__GAME__", game_dir);
    html = html.replace("__LIBRARY__", library_label);
    html = html.replace("__LEGACY__", legacy_note);
    html
}

const TEMPLATE: &str = r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>资产目录 · art-catalog</title>
<style>
:root { --bg:#14171c; --panel:#1d2229; --line:#2c333d; --fg:#dfe5ec; --dim:#8b95a3;
        --ok:#7aa25c; --warn:#e8a33d; --err:#d64541; --info:#6b9fd6; --accent:#ffd23f; }
/* refs view (card AC-5): code anchor → model → real clip */
.refgroup{position:relative;display:grid;grid-template-columns:1fr auto 1fr;gap:28px;align-items:center;border:1px solid var(--line);border-radius:10px;padding:14px;margin-bottom:14px;background:var(--panel)}
.refgroup.orphan{border-color:var(--err)}
.reflines{position:absolute;inset:0;width:100%;height:100%;pointer-events:none}
.refcol{display:flex;flex-direction:column;gap:6px;min-width:0;z-index:1}
.refcol.left{align-items:flex-end}
.refcol.right{align-items:flex-start}
.refchip{background:var(--bg);border:1px solid var(--line);border-radius:6px;padding:3px 9px;font-size:12px}
.refchip.animsym{border-color:var(--info)}
.refchip.dim{opacity:.5}
.refmodel{border:2px solid var(--accent);border-radius:10px;padding:10px 16px;cursor:pointer;text-align:center;min-width:130px;background:var(--bg)}
.refgroup.orphan .refmodel{border-color:var(--err)}
.clipchip{border:1px solid var(--line);border-radius:6px;padding:3px 9px;font-size:12px;background:var(--bg)}
.clipchip.used{border-color:var(--ok);color:var(--ok)}
.clipchip.dim{opacity:.4}
.clipchip.mismatch{border-color:var(--err);color:var(--err)}
.refchip.animsym.bad{border-color:var(--err);color:var(--err)}
.reflines .l-model{fill:none;stroke:var(--dim);stroke-width:1.5;opacity:.55}
.reflines .l-clip{fill:none;stroke:var(--ok);stroke-width:1.5;opacity:.7}
* { box-sizing:border-box; }
body { margin:0; background:var(--bg); color:var(--fg); font:14px/1.5 "Segoe UI",system-ui,sans-serif; }
header { padding:14px 20px; border-bottom:1px solid var(--line); display:flex; flex-wrap:wrap; gap:10px; align-items:baseline; }
header h1 { font-size:18px; margin:0; }
header .meta { color:var(--dim); font-size:12px; }
.chips { display:flex; flex-wrap:wrap; gap:8px; padding:12px 20px; }
.chip { background:var(--panel); border:1px solid var(--line); border-radius:8px; padding:6px 12px; font-size:13px; }
.chip b { font-size:16px; margin-right:4px; }
.chip.ok b { color:var(--ok); } .chip.warn b { color:var(--warn); } .chip.err b { color:var(--err); } .chip.info b { color:var(--info); }
nav { display:flex; flex-wrap:wrap; gap:4px; padding:0 20px; border-bottom:1px solid var(--line); position:sticky; top:0; background:var(--bg); z-index:5; }
nav button { background:none; border:none; color:var(--dim); padding:10px 14px; cursor:pointer; font-size:14px; border-bottom:2px solid transparent; }
nav button.active { color:var(--fg); border-bottom-color:var(--accent); }
section.view { display:none; padding:16px 20px; }
section.view.active { display:block; }
.toolbar { display:flex; flex-wrap:wrap; gap:8px; margin-bottom:12px; align-items:center; }
.toolbar input, .toolbar select { background:var(--panel); color:var(--fg); border:1px solid var(--line); border-radius:6px; padding:6px 10px; }
.toggle button { background:var(--panel); color:var(--dim); border:1px solid var(--line); padding:6px 10px; cursor:pointer; }
.toggle button:first-child { border-radius:6px 0 0 6px; }
.toggle button:last-child { border-radius:0 6px 6px 0; border-left:none; }
.toggle button.on { background:#2c3a4d; color:var(--fg); }
table { width:100%; border-collapse:collapse; background:var(--panel); border-radius:8px; overflow:hidden; }
th, td { padding:8px 10px; border-bottom:1px solid var(--line); text-align:left; font-size:13px; vertical-align:top; }
th { color:var(--dim); font-weight:600; cursor:default; white-space:nowrap; }
tr:hover td { background:#232a33; }
.badge { display:inline-block; padding:1px 8px; border-radius:10px; font-size:12px; margin:1px 2px; }
.b-ok { background:#26331f; color:var(--ok); } .b-warn { background:#3d3220; color:var(--warn); }
.b-err { background:#3d2220; color:var(--err); } .b-info { background:#20303d; color:var(--info); }
.b-dim { background:#252b33; color:var(--dim); }
.gallery { display:grid; grid-template-columns:repeat(auto-fill,minmax(210px,1fr)); gap:14px; }
.gcard { background:var(--panel); border:1px solid var(--line); border-radius:10px; overflow:hidden; cursor:pointer; }
.gcard img { width:100%; aspect-ratio:1; object-fit:cover; display:block; background:#0f1216; }
.gcard .cap { padding:8px 10px; }
.gcard .cap b { display:block; }
.gcard .cap span { color:var(--dim); font-size:12px; }
.gcard .cardcover { width:100%; aspect-ratio:1; background-repeat:no-repeat; background-color:#0f1216; }
#lightbox { position:fixed; inset:0; background:rgba(0,0,0,.85); display:none; z-index:50; overflow:auto; padding:30px; }
#lightbox .inner { max-width:980px; margin:0 auto; background:var(--panel); border-radius:12px; padding:20px; }
#lightbox .views { display:grid; grid-template-columns:repeat(auto-fill,minmax(200px,1fr)); gap:10px; }
#lightbox img { width:100%; border-radius:8px; }
#lightbox table { margin-top:12px; }
.animrow { display:flex; gap:10px; align-items:center; margin:6px 0; }
.animrow .sprite { width:160px; height:160px; background-repeat:no-repeat; background-color:#0f1216; border-radius:8px; }
.animrow span { color:var(--dim); font-size:12px; }
.v3dwrap { background:#12161b; border:1px solid var(--line); border-radius:10px; padding:10px; margin-top:8px; }
#v3d-canvas { width:100%; height:420px; display:block; border-radius:8px; background:#12161b; touch-action:none; }
.v3dctl { display:flex; gap:10px; align-items:center; margin-top:8px; flex-wrap:wrap; }
.import-guide { background:var(--panel); border:1px solid var(--line); border-radius:10px; padding:12px 14px; margin:8px 0 10px; line-height:1.9; }
.import-guide code { background:#0f1216; }
button.copybtn { background:#2c3a4d; color:var(--fg); border:1px solid var(--line); border-radius:6px; padding:6px 12px; cursor:pointer; }
.sev-error { color:var(--err); font-weight:700; } .sev-warning { color:var(--warn); } .sev-info { color:var(--info); }
code { background:#0f1216; padding:1px 5px; border-radius:4px; font-size:12px; }
.note { color:var(--dim); font-size:12px; }
.empty { color:var(--dim); padding:30px; text-align:center; }
h2 { font-size:15px; margin:18px 0 8px; color:var(--dim); }
</style>
</head>
<body>
<header>
  <h1>资产目录 · art-catalog</h1>
  <span class="meta">游戏：<span id="hdr-game"></span> ｜ 库：<span id="hdr-lib"></span> ｜ 生成于 __GENERATED__ ｜ <span id="hdr-legacy"></span></span>
</header>
<div class="chips" id="chips"></div>
<nav id="tabs"></nav>
<section class="view" id="view-overview"><div id="ov"></div></section>
<section class="view" id="view-assets">
  <div class="toolbar">
    <span class="toggle" id="as-dom"></span>
    <span class="toggle" id="as-kind"></span>
    <input id="as-q" placeholder="搜索名称/路径…">
    <select id="as-sort">
      <option value="name">按名称</option><option value="height">按身高</option>
      <option value="tris">按三角数</option><option value="clips">按 clip 数</option>
    </select>
  </div>
  <div id="assets"></div>
</section>
<section class="view" id="view-gallery">
  <div class="toolbar"><span class="toggle" id="gal-dom"></span><input id="gal-q" placeholder="搜索名称…"><label class="note"><input type="checkbox" id="gal-anim"> 只看有动画</label><label class="note"><input type="checkbox" id="gal-raw"> 显示无图册原件（raw）</label></div>
  <div class="gallery" id="gal"></div>
</section>
<section class="view" id="view-refs"><div id="refv"></div></section>
<section class="view" id="view-report"><div id="rep"></div></section>
<div id="lightbox" onclick="this.style.display='none'"><div class="inner" onclick="event.stopPropagation()">
  <div id="v3d-persist" style="display:none">
    <div class="v3dwrap">
      <canvas id="v3d-canvas"></canvas>
      <div class="v3dctl">
        <select id="v3d-clip"></select>
        <button id="v3d-play" class="copybtn">⏸ 暂停</button>
        <label class="note">速度 <input type="range" id="v3d-speed" min="0.25" max="2" step="0.25" value="1"></label>
        <input type="range" id="v3d-timeline" min="0" max="1000" value="0" style="flex:1">
        <span class="note" id="v3d-time">0.00s</span>
      </div>
      <div class="note">拖拽旋转 · 滚轮缩放 · 右键平移　<span id="v3d-status"></span></div>
    </div>
  </div>
  <div id="lb-body"></div>
</div></div>
<script>__VENDOR3D__</script>
<script>
const DATA = __DATA__;
const ROOT = "__ROOT__";
const src = p => ROOT ? ROOT + "/" + p : p;

const TABS = [["overview","总览与导入"],["assets","资产"],["gallery","图册"],["refs","引用"],["report","检查"]];
const stats = DATA.stats;

document.getElementById("hdr-game").textContent = DATA.game;
document.getElementById("hdr-lib").textContent = DATA.library || "（无，仅游戏域）";
document.getElementById("hdr-legacy").innerHTML = DATA.legacy ? "<span class='sev-warning'>迁移前布局（_art/）：尚未建 _library/</span>" : "工作区级库布局";
const chipDefs = [
  ["库候选", stats.library_candidates, "info"], ["库成品货架", stats.library_washed, "info"],
  ["游戏运行时模型", stats.game_models, ""], ["被引用", stats.referenced, "ok"],
  ["孤儿", stats.orphans, "err"], ["过时", stats.stale, "warn"],
  ["检查问题", stats.findings, "err"], ["待处理工单", stats.intake_open, "warn"],
];
document.getElementById("chips").innerHTML = chipDefs.map(([l,v,c]) =>
  `<div class="chip ${c}"><b>${v}</b>${l}</div>`).join("");

const nav = document.getElementById("tabs");
nav.innerHTML = TABS.map(([id,label],i) => `<button data-t="${id}" class="${i===0?'active':''}">${label}</button>`).join("");
nav.querySelectorAll("button").forEach(b => b.onclick = () => {
  nav.querySelectorAll("button").forEach(x => x.classList.remove("active"));
  b.classList.add("active");
  document.querySelectorAll("section.view").forEach(s => s.classList.remove("active"));
  document.getElementById("view-" + b.dataset.t).classList.add("active");
  if (b.dataset.t === "refs") renderRefs();
});
document.getElementById("view-overview").classList.add("active");

function toggleHTML(id, opts, onPick) {
  const el = document.getElementById(id);
  el.innerHTML = opts.map(([v,l],i) => `<button data-v="${v}" class="${i===0?'on':''}">${l}</button>`).join("");
  el.querySelectorAll("button").forEach(b => b.onclick = () => {
    el.querySelectorAll("button").forEach(x => x.classList.remove("on"));
    b.classList.add("on"); onPick(b.dataset.v);
  });
}
const models = DATA.assets.filter(a => a.kind === "model");
const fmtSize = n => n > 1048576 ? (n/1048576).toFixed(1)+" MiB" : (n/1024).toFixed(0)+" KiB";
const esc = s => String(s??"").replace(/[&<>"]/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;"}[c]));
// ground-truth animation facts straight from the glb container (a.glb), with
// meta.json clips as fallback; empty names shown as 未命名
const glbClips = a => {
  const g = a.glb;
  if (g) return { n: g.animations.length, names: g.animations.map(x => x || "（未命名）"), truth: true };
  if (a.meta?.clips?.length) return { n: a.meta.clips.length, names: a.meta.clips, truth: false };
  return { n: null, names: [], truth: false };
};

// ---------- assets (models rich table + all-assets list) ----------
let asDomain = "all", asKind = "model", asQ = "", asSort = "name";
function renderAssets() {
  let rows = DATA.assets.filter(a => (asDomain==="all" || a.domain===asDomain) &&
    (!asQ || a.path.toLowerCase().includes(asQ)));
  if (asKind === "model") rows = rows.filter(a => a.kind === "model");
  const keyFns = {
    name: a => a.id, height: a => a.meta?.height_m ?? -1,
    tris: a => a.meta?.triangles ?? -1, clips: a => glbClips(a).n ?? -1,
  };
  const kf = keyFns[asSort];
  rows.sort((x,y) => { const kx = kf(x), ky = kf(y); return (kx<ky?-1:kx>ky?1:0) || x.id.localeCompare(y.id); });
  const el = document.getElementById("assets");
  if (!rows.length) { el.innerHTML = `<div class="empty">无匹配</div>`; return; }
  if (asKind === "model") {
    const PIPE = { landed:["已上架","b-ok"], review:["待拍板","b-warn"], washing:["洗白中","b-info"], new:["已立案","b-dim"], rejected:["已拒","b-err"] };
    const pipeCell = a => {
      const s = a.pipeline_status;
      if (s && PIPE[s]) return `<span class="badge ${PIPE[s][1]}">${PIPE[s][0]}</span>`;
      return a.domain==="library" ? `<span class="badge b-dim">未立案</span>` : "—";
    };
    el.innerHTML = `<table><tr><th>模型</th><th>域</th><th>管线</th><th>实际 clips</th><th>身高 m</th><th>三角</th><th>引用</th><th>状态</th></tr>` +
      rows.map(a => {
        const m = a.meta, g = glbClips(a);
        const badges = [];
        badges.push(a.refs.length ? `<span class="badge b-ok">引用×${a.refs.length}</span>` : (a.domain==="game" ? `<span class="badge b-err">孤儿</span>` : `<span class="badge b-dim">未上架</span>`));
        (a.stale_reasons||[]).forEach(() => badges.push(`<span class="badge b-warn">过时</span>`));
        if (a.domain==="game" && !m) badges.push(`<span class="badge b-warn">缺 meta</span>`);
        const clipsCell = g.n === null ? "—" :
          `<span title="${esc(g.names.join(" / "))}">${g.n} 条</span>${g.truth ? "" : ` <span class="note">(meta)</span>`}`;
        return `<tr>
          <td><a href="#" onclick="showAsset('${esc(a.id)}');return false"><b>${esc(a.id)}</b></a><div class="note">${esc(a.path)}</div></td>
          <td>${a.domain==="library"?"库":"游戏"}</td>
          <td>${pipeCell(a)}</td>
          <td>${clipsCell}</td>
          <td>${m?.height_m ?? "—"}</td><td>${m?.triangles ?? "—"}</td>
          <td>${a.refs.length ? a.refs.map(r=>`<div><code>${esc(r.file)}:${r.line}</code></div>`).join("") : "—"}</td>
          <td>${badges.join("")}</td></tr>`;
      }).join("") + `</table>`;
  } else {
    el.innerHTML = `<table><tr><th>路径</th><th>域</th><th>类型</th><th>大小</th><th>引用</th><th>预览</th></tr>` +
      rows.map(a => {
        let pv = "";
        const ext = a.path.split(".").pop().toLowerCase();
        if (["png","jpg","jpeg","webp","svg"].includes(ext))
          pv = `<img loading="lazy" src="${src(a.path)}" style="max-width:72px;max-height:48px;border-radius:4px">`;
        else if (["mp3","ogg","wav","flac"].includes(ext))
          pv = `<audio controls preload="none" src="${src(a.path)}" style="height:28px"></audio>`;
        const refBadge = a.refs.length ? `<span class="badge b-ok">×${a.refs.length}</span>`
          : (a.domain==="game" && a.kind!=="other" ? `<span class="badge b-err">孤儿</span>` : `<span class="badge b-dim">—</span>`);
        const open = a.kind==="model" ? ` <a href="#" onclick="showAsset('${esc(a.id)}');return false" class="note">详情</a>` : "";
        return `<tr><td><code>${esc(a.path)}</code>${open}</td><td>${a.domain==="library"?"库":"游戏"}</td>
          <td>${a.kind}</td><td>${fmtSize(a.size)}</td><td>${refBadge}</td><td>${pv}</td></tr>`;
      }).join("") + `</table>`;
  }
}
toggleHTML("as-dom", [["all","全部"],["library","库"],["game","游戏"]], v => { asDomain = v; renderAssets(); });
toggleHTML("as-kind", [["model","模型"],["all","全类型"]], v => { asKind = v; renderAssets(); });
document.getElementById("as-q").oninput = e => { asQ = e.target.value.toLowerCase(); renderAssets(); };
document.getElementById("as-sort").onchange = e => { asSort = e.target.value; renderAssets(); };

// ---------- gallery ----------
let galDomain = "all", galQ = "", galAnimOnly = false, galRaw = false;
function renderGallery() {
  let items = models.filter(a => (galDomain==="all" || a.domain===galDomain) && (!galQ || a.id.toLowerCase().includes(galQ)));
  if (galAnimOnly) items = items.filter(a => (a.meta?.anim||[]).length || glbClips(a).n);
  // raw 原件没有渲染图（渲染只在上架洗白时生成），默认不混进图册
  if (!galRaw) items = items.filter(a => a.meta?.renders?.length);
  document.getElementById("gal").innerHTML = !items.length ? `<div class="empty">无匹配</div>` :
    items.map(a => {
      const cover = a.meta?.renders?.[0];
      const sub = a.meta ? `h=${a.meta.height_m}m · ${a.meta.triangles?? "?"} tri` : fmtSize(a.size);
      const gn = glbClips(a);
      const animBadge = gn.n ? `<span class="badge b-info">▶ ${gn.n} 动画</span>` : "";
      // pipeline badge (card AC-3): raw originals get "raw 原件"; shelf items
      // get 已上架 (in game) or 候选 (washed, not yet promoted)
      const rawBadge = a.meta?.renders?.length ? "" : ` <span class="badge b-dim">raw 原件</span>`;
      const shelveBadge = a.meta?.renders?.length
        ? ` <span class="badge ${a.domain==="game" ? "b-ok" : "b-info"}">${a.domain==="game" ? "已上架" : "候选"}</span>`
        : "";
      // cover = the model's static turntable render (human feedback #4)
      const inner = cover ? `<img loading="lazy" src="${src(cover)}" alt="${esc(a.id)}">` : `<div class="empty" style="aspect-ratio:1">无图册（未洗白）</div>`;
      return `<div class="gcard" onclick="showAsset('${esc(a.id)}')">${inner}<div class="cap"><b>${esc(a.id)}</b><span>${a.domain==="library"?"库候选/成品":"游戏运行时"} · ${esc(sub)}</span>${animBadge}${rawBadge}${shelveBadge}</div></div>`;
    }).join("");
  startSprites();
}
toggleHTML("gal-dom", [["all","全部"],["library","库"],["game","游戏"]], v => { galDomain = v; renderGallery(); });
document.getElementById("gal-q").oninput = e => { galQ = e.target.value.toLowerCase(); renderGallery(); };
document.getElementById("gal-anim").onchange = e => { galAnimOnly = e.target.checked; renderGallery(); };
document.getElementById("gal-raw").onchange = e => { galRaw = e.target.checked; renderGallery(); };

// ---------- refs view (card AC-5): code anchor → model → real clip ----------
function renderRefs() {
  const gm = models.filter(a => a.domain === "game" && a.kind === "model");
  const el = document.getElementById("refv");
  if (!gm.length) { el.innerHTML = `<div class="empty">无游戏模型</div>`; return; }
  el.innerHTML = `<p class="note">左=代码锚点（📄 模型字面量 / ⚡ 动画索引符号） · 中=运行时模型 · 右=文件真实动画（glb 顺序 + 时长指纹）。灰色 = 代码无引用（死动画）；红 ⚠ = 索引处名字对不上（错位）；整组红框 = 孤儿模型。点模型名开详情。</p>` + gm.map(a => {
    const anims = a.glb?.animations || [];
    const durs = a.glb?.durations || [];
    const used = new Map();
    (a.anim_refs||[]).forEach(r => {
      if (!used.has(r.clip_index)) used.set(r.clip_index, []);
      used.get(r.clip_index).push(`${r.symbol} (${r.file.split("/").pop()}:${r.line})`);
    });
    const mm = refMismatches(a);
    const mmIdx = new Set(mm.map(m => m.clip_index));
    const mmSyms = new Set(mm.map(m => `${m.clip_index}|${m.symbol}|${m.line}`));
    const fileRefs = [...new Set((a.refs||[]).map(r => `${r.file.split("/").pop()}:${r.line}`))];
    const left = [
      ...fileRefs.map(f => `<div class="refchip">📄 ${esc(f)}</div>`),
      ...(a.anim_refs||[]).map(r => `<div class="refchip animsym ${mmSyms.has(`${r.clip_index}|${r.symbol}|${r.line}`)?"bad":""}">⚡ ${esc(r.symbol)}=${r.clip_index} <span class="note">${esc(r.file.split("/").pop())}:${r.line}</span></div>`)
    ].join("") || `<div class="refchip dim">无代码引用</div>`;
    const orphan = !a.refs.length;
    const right = anims.length
      ? anims.map((nm,i) => {
          const dur = durs[i];
          const syms = used.get(i);
          const bad = mmIdx.has(i);
          const cls = bad ? "mismatch" : (syms ? "used" : "dim");
          const mark = bad ? `⚠≠${esc(mm.filter(m=>m.clip_index===i).map(m=>clipToken(m.symbol)).join("/"))}` : (syms?"✓":"·");
          return `<div class="clipchip ${cls}">${esc(nm||"（未命名）")}${dur?` ${dur.toFixed(2)}s`:""} ${mark}</div>`;
        }).join("")
      : ((a.meta?.anim||[]).map(c => `<div class="clipchip dim">${esc(c.name)} ·</div>`).join("") || `<div class="clipchip dim">无动画</div>`);
    const warn = mm.length ? `<span class="badge b-err">⚠ ${mm.length} 处索引-名字错位</span><br>` : "";
    return `<div class="refgroup ${orphan?"orphan":""}">
      <svg class="reflines"></svg>
      <div class="refcol left">${left}</div>
      <div class="refmodel" onclick="showAsset('${esc(a.id)}')"><b>${esc(a.id)}</b><br>${warn}<span class="note">${orphan?"孤儿（无代码引用）":`引用×${a.refs.length} · 动画×${anims.length}`}</span></div>
      <div class="refcol right">${right}</div>
    </div>`;
  }).join("");
  requestAnimationFrame(drawRefLines);
}
function drawRefLines() {
  document.querySelectorAll(".refgroup").forEach(g => {
    const svg = g.querySelector("svg.reflines");
    const node = g.querySelector(".refmodel");
    if (!svg || !node) return;
    const gb = g.getBoundingClientRect(), nb = node.getBoundingClientRect();
    svg.setAttribute("viewBox", `0 0 ${gb.width} ${gb.height}`);
    let d = "";
    const nx = nb.left - gb.left, ny = nb.top - gb.top + nb.height/2;
    g.querySelectorAll(".refcol.left .refchip").forEach(c => {
      if (c.classList.contains("dim")) return;
      const cb = c.getBoundingClientRect();
      const x1 = cb.right - gb.left, y1 = cb.top - gb.top + cb.height/2;
      d += `<path d="M${x1},${y1} C${x1+22},${y1} ${nx-22},${ny} ${nx},${ny}" class="l-model"/>`;
    });
    g.querySelectorAll(".refcol.right .clipchip.used").forEach(c => {
      const cb = c.getBoundingClientRect();
      const x2 = cb.left - gb.left, y2 = cb.top - gb.top + cb.height/2;
      d += `<path d="M${nx+nb.width},${ny} C${nx+nb.width+22},${ny} ${x2-22},${y2} ${x2},${y2}" class="l-clip"/>`;
    });
    svg.innerHTML = d;
  });
}
window.addEventListener("resize", () => { if (document.getElementById("view-refs").classList.contains("active")) drawRefLines(); });

// ---------- 3D viewer (three.js r147 inlined) ----------
// The canvas lives in #v3d-persist and is NEVER destroyed on re-render;
// destroying it detaches the WebGL context and kills playback (the bug where
// only the first opened model animated).
const V3D = { inited:false, id:null, model:null, mixer:null, actions:{}, keys:[], labels:[], current:null, paused:false, scrubbing:false };
function v3dInit() {
  if (V3D.inited) return;
  const canvas = document.getElementById("v3d-canvas");
  const w = canvas.clientWidth || 800, h = 420;
  V3D.renderer = new THREE.WebGLRenderer({ canvas, antialias: true });
  V3D.renderer.setSize(w, h, false);
  V3D.renderer.outputEncoding = THREE.sRGBEncoding;
  V3D.scene = new THREE.Scene();
  V3D.scene.background = new THREE.Color(0x161a20);
  V3D.camera = new THREE.PerspectiveCamera(45, w / h, 0.01, 100);
  V3D.camera.position.set(1.6, 1.2, 2.4);
  V3D.scene.add(new THREE.HemisphereLight(0xffffff, 0x36414e, 1.05));
  const d = new THREE.DirectionalLight(0xffffff, 1.4); d.position.set(2, 4, 3); V3D.scene.add(d);
  V3D.grid = new THREE.GridHelper(10, 20, 0x3a4450, 0x272e38); V3D.scene.add(V3D.grid);
  V3D.controls = new THREE.OrbitControls(V3D.camera, canvas);
  V3D.clock = new THREE.Clock();
  V3D.inited = true;
  new ResizeObserver(() => {
    const cw = canvas.clientWidth, ch = canvas.clientHeight || 420;
    if (!cw) return;
    V3D.renderer.setSize(cw, ch, false);
    V3D.camera.aspect = cw / ch; V3D.camera.updateProjectionMatrix();
  }).observe(canvas);
  document.getElementById("v3d-play").onclick = () => {
    V3D.paused = !V3D.paused;
    document.getElementById("v3d-play").textContent = V3D.paused ? "▶ 播放" : "⏸ 暂停";
  };
  document.getElementById("v3d-speed").oninput = e => { if (V3D.mixer) V3D.mixer.timeScale = +e.target.value; };
  const tl = document.getElementById("v3d-timeline");
  tl.oninput = () => { V3D.scrubbing = true; };
  tl.onchange = () => {
    if (V3D.current) {
      const dur = V3D.current.getClip().duration;
      V3D.current.time = (+tl.value) / 1000 * dur;
      if (V3D.mixer) V3D.mixer.update(0);
    }
    V3D.scrubbing = false;
  };
  document.getElementById("v3d-clip").onchange = e => v3dSelectClip(e.target.value);
  (function loop() {
    requestAnimationFrame(loop);
    const lb = document.getElementById("lightbox");
    const vp = document.getElementById("v3d-persist");
    if (!lb || lb.style.display !== "block" || !vp || vp.style.display === "none") return;
    const dt = V3D.clock.getDelta();
    if (V3D.mixer && !V3D.paused) V3D.mixer.update(dt);
    V3D.controls.update();
    V3D.renderer.render(V3D.scene, V3D.camera);
    const t = document.getElementById("v3d-timeline"), tm = document.getElementById("v3d-time");
    if (V3D.current && t && !V3D.scrubbing) {
      const dur = V3D.current.getClip().duration;
      t.value = Math.round(V3D.current.time / dur * 1000);
      tm.textContent = V3D.current.time.toFixed(2) + "s / " + dur.toFixed(2) + "s";
    }
  })();
}
function v3dLoad(id, preferredClip) {
  v3dInit();
  const st = document.getElementById("v3d-status");
  if (V3D.id === id) {
    v3dSelectClip(preferredClip !== undefined && V3D.keys.includes(preferredClip) ? preferredClip : V3D.keys[0]);
    return;
  }
  if (V3D.model) { V3D.scene.remove(V3D.model); V3D.model = null; }
  V3D.mixer = null; V3D.current = null; V3D.actions = {}; V3D.keys = []; V3D.labels = []; V3D.id = id;
  st.textContent = "加载模型数据中…";
  const s = document.createElement("script");
  s.src = "modeldata/" + id + ".js";
  s.onload = () => {
    const b64 = (window.__ART_MODELS || {})[id];
    if (!b64) { st.textContent = "模型数据缺失"; return; }
    const buf = Uint8Array.from(atob(b64), c => c.charCodeAt(0)).buffer;
    new THREE.GLTFLoader().parse(buf, "", gltf => {
      const model = gltf.scene;
      // feet on the ground (human feedback #5): bottom of the bounding box at
      // y=0, horizontally centered — NOT bbox-center at origin (that puts the
      // grid through the model's belly)
      const box = new THREE.Box3().setFromObject(model);
      const c = box.getCenter(new THREE.Vector3()), sz = box.getSize(new THREE.Vector3());
      model.position.x -= c.x;
      model.position.z -= c.z;
      model.position.y -= box.min.y;
      const h = Math.max(sz.y, 0.0001);
      const r = Math.max(sz.x, sz.y, sz.z) * 0.5 || 1;
      V3D.camera.position.set(r * 1.5, h * 0.8, r * 2.2);
      V3D.camera.near = r / 100; V3D.camera.far = r * 40; V3D.camera.updateProjectionMatrix();
      V3D.controls.target.set(0, h * 0.5, 0);
      V3D.grid.scale.setScalar(Math.max(r / 2.5, 0.2));
      V3D.scene.add(model); V3D.model = model;
      V3D.mixer = new THREE.AnimationMixer(model);
      // key clips by INDEX (file names may be empty or duplicated)
      gltf.animations.forEach((x, i) => {
        const key = String(i);
        V3D.keys.push(key);
        V3D.labels.push(x.name || `clip${gltf.animations.length > 1 ? i + 1 : ""}（未命名）`);
        V3D.actions[key] = V3D.mixer.clipAction(x);
      });
      document.getElementById("v3d-clip").innerHTML =
        V3D.keys.map((k, i) => `<option value="${k}">${esc(V3D.labels[i])}</option>`).join("");
      st.textContent = gltf.animations.length ?
        `已加载 ${gltf.animations.length} 条 clip` : "⚠️ 此文件不含动画数据";
      v3dSelectClip(V3D.keys[0]);
    }, err => { st.textContent = "解析失败：" + err; });
  };
  s.onerror = () => { st.textContent = "modeldata/" + id + ".js 缺失（模型可能过大未内嵌）"; };
  document.body.appendChild(s);
}
function v3dSelectClip(key) {
  if (!V3D.mixer || key === undefined || key === null) return;
  if (V3D.current) V3D.current.stop();
  const a = V3D.actions[key];
  if (!a) return;
  a.reset().setLoop(THREE.LoopRepeat).play();
  V3D.current = a; V3D.paused = false;
  document.getElementById("v3d-play").textContent = "⏸ 暂停";
  document.getElementById("v3d-clip").value = key;
}
// clip truth helpers (card AC-5 hardening): expected name token from symbol,
// and index↔name mismatch list against the glb's own animations
function clipToken(sym) {
  const s = (sym||"").toLowerCase().replace(/_clip$/,"");
  const parts = s.split("_").filter(Boolean);
  return parts.length ? parts[parts.length-1] : "";
}
function refMismatches(a) {
  const g = a.glb; if (!g) return [];
  const out = [];
  (a.anim_refs||[]).forEach(r => {
    const name = (g.animations||[])[r.clip_index];
    if (name === undefined) { out.push({...r, over:true}); return; }
    if (!name) return;
    const tok = clipToken(r.symbol);
    if (!tok) return;
    const nl = name.toLowerCase(), tl = tok.toLowerCase();
    if (!(nl===tl || nl.includes(tl) || tl.includes(nl))) out.push({...r, name, dur:(g.durations||[])[r.clip_index]});
  });
  return out;
}
function showAsset(id, preferredClip) {
  const a = models.find(x => x.id === id); if (!a) return;
  const m = a.meta;
  const embeddable = (DATA.embedded || []).includes(a.id);
  const views = (m?.renders||[]).map(r => `<img src="${src(r)}">`).join("");
  const anims = (m?.anim||[]).map(c => {
    const gnames = a.glb?.animations || [];
    const syms = (a.anim_refs||[]).filter(r => (gnames[r.clip_index]||"") === c.name).map(r => r.symbol);
    const mark = syms.length ? `<span class="badge b-ok">✓ ${esc(syms.join(","))}</span>` : `<span class="badge b-dim">无引用</span>`;
    return `<div class="animrow"><div class="sprite" data-strip="${src(c.strip)}" data-frames="${c.frames}" data-i="0"></div><span>${esc(c.name)}</span>${mark}</div>`;
  }).join("");
  const animRefHtml = (() => {
    const refs = a.anim_refs || [];
    if (!refs.length) return null;
    const anims = a.glb?.animations || [];
    const durs = a.glb?.durations || [];
    const mmKeys = new Set(refMismatches(a).map(m => `${m.clip_index}|${m.symbol}|${m.line}`));
    return refs.map(r => {
      const nm = anims[r.clip_index];
      const dur = durs[r.clip_index];
      const bad = mmKeys.has(`${r.clip_index}|${r.symbol}|${r.line}`);
      const target = nm !== undefined
        ? `${nm}${dur?`（${dur.toFixed(2)}s）`:""}${bad?" ⚠ 错位":""}`
        : `#${r.clip_index}（超出文件动画数！）`;
      return `${target} ← <code>${esc(r.symbol)}</code> (${esc(r.file)}:${r.line})`;
    }).join("<br>");
  })();
  document.getElementById("v3d-persist").style.display = embeddable ? "block" : "none";
  document.getElementById("lb-body").innerHTML = `<h2 style="color:var(--fg)">${esc(a.id)} <span class="note">${esc(a.path)}</span></h2>
    <div class="views">${views || `<div class="empty">无图册渲染</div>`}</div>
    ${anims ? `<h2>动图条（8fps 循环）</h2>${anims}` : ""}
    <table>${[["来源",m?.source],["身高",m?.height_m&&m.height_m+" m"],["三角数",m?.triangles],["材质",m?.materials?.join(", ")],
      ["骨骼",m?.has_armature?"有":"无"],
      ["文件实际动画", a.glb ? (a.glb.animations.length ? a.glb.animations.map(x=>x||"（未命名）").join(", ") : "无（文件不含动画）") : null],
      ["动画引用", animRefHtml],
      ["绑定（代码引用）", a.refs.length ? a.refs.map(r=>`${r.file}:${r.line}`).join("；") : null],
      ["meta",m?.meta_path]]
      .map(([k,v]) => v?`<tr><th style="width:110px">${k}</th><td>${esc(v)}</td></tr>`:"").join("")}</table>
    ${embeddable ? "" : `<div class="note">3D 视图不可用（未内嵌），动图见上。</div>`}`;
  if (embeddable) v3dLoad(a.id, preferredClip);
  startSprites();
  document.getElementById("lightbox").style.display = "block";
}
// sprite-strip loops (~8fps); strip N frames -> background-position steps i*100/(N-1) %
function startSprites() {
  document.querySelectorAll(".sprite:not([style*='background-image'])").forEach(el => {
    el.style.backgroundImage = `url("${el.dataset.strip}")`;
  });
}
let lastT = 0;
function animTick(t) {
  if (t - lastT > 120) {
    lastT = t;
    document.querySelectorAll(".sprite").forEach(el => {
      const f = +el.dataset.frames;
      if (f < 2) return;
      const i = ((+el.dataset.i) + 1) % f;
      el.dataset.i = i;
      el.style.backgroundSize = `${f * 100}% 100%`;
      el.style.backgroundPosition = `${(i * 100 / (f - 1)).toFixed(2)}% 0`;
    });
  }
  requestAnimationFrame(animTick);
}
requestAnimationFrame(animTick);

// ---------- report ----------
function renderReport() {
  const fs = DATA.findings;
  const byRule = {};
  fs.forEach(x => (byRule[x.rule_id] = byRule[x.rule_id] || []).push(x));
  const scenNote = `<div class="note" style="margin:6px 0 12px">修复按场景卡走（人批准，AI 执行）：SC6 体检=逐条修；触发语与工单说明见「总览与导入」。完整卡：tools/art-catalog/scenarios/。</div>`;
  document.getElementById("rep").innerHTML = scenNote + (!fs.length ? `<div class="empty">全绿，无检查问题 ✅</div>` :
    Object.keys(byRule).sort().map(rule => `<h2>${rule}（${byRule[rule].length}）</h2><table><tr><th>级别</th><th>对象</th><th>证据</th><th>修复建议</th></tr>` +
      byRule[rule].map(x => `<tr><td><span class="sev-${x.severity}">${x.severity}</span></td>
        <td><code>${esc(x.subject)}</code></td><td>${esc(x.evidence)}</td><td class="note">${esc(x.fix_hint)}</td></tr>`).join("") + `</table>`).join(""));
}

// ---------- overview: import first, then health ----------
function copyImportGuide(btn) {
  const text = "请按场景卡 SC1 导入美术资产：raw 路径=games/wave-survival/_art/raw/<文件名>，目标身高=<米>；用 art-catalog intake 落单再执行，洗到 review 后等我翻图册拍板。（终端等价：art-catalog wash --file games/wave-survival/_art/raw/<文件名> --height <米> --license <许可> --blender D:\\\\Blender\\\\blender.exe）";
  const done = () => { btn.textContent = "已复制 ✓"; setTimeout(() => btn.textContent = "复制导入提示语模板", 1500); };
  if (navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(text).then(done).catch(() => prompt("手动复制：", text));
  } else { prompt("手动复制：", text); }
}
function copyImportFile(btn, name) {
  const rawDir = (DATA.library || DATA.game + "/_art") + "/raw";
  const text = `请按场景卡 SC1 导入美术资产：raw 路径=${rawDir}/${name}，目标身高=<米>；用 art-catalog intake 落单再执行，洗到 review 后等我翻图册拍板。（终端等价：art-catalog wash --file ${rawDir}/${name} --height <米> --license <许可> --blender D:\\\\Blender\\\\blender.exe）`;
  const done = () => { btn.textContent = "已复制 ✓"; setTimeout(() => btn.textContent = "复制这句", 1500); };
  if (navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(text).then(done).catch(() => prompt("手动复制：", text));
  } else { prompt("手动复制：", text); }
}
function wireImport() {
  const btn = document.getElementById("import-btn"), fi = document.getElementById("import-file");
  if (!btn || !fi) return;
  const statusEl = () => document.getElementById("import-status");
  const rawDir = (DATA.library || DATA.game + "/_art") + "/raw";
  const promptBox = name => `<div class="import-guide" style="margin-top:8px">对 AI 说：<code>按 SC1 把 raw/${esc(name)} 洗白入库，目标身高 &lt;米&gt;</code>
      　<button class="copybtn" onclick="copyImportFile(this,'${esc(name)}')">复制这句</button><br>或终端一键（AC-4）：<code>art-catalog wash --file ${esc(rawDir)}/${esc(name)} --height &lt;米&gt; --license &lt;许可&gt; --blender D:\\\\Blender\\\\blender.exe</code></div>`;
  // File System Access API needs a real user gesture for EACH picker, and
  // Chromium consumes the activation for the first one. So the directory grant
  // runs on the button click (gesture #1); the file picker runs on a later
  // click (gesture #2) using the already-granted handle. Doing both in the
  // file-input `change` handler loses the gesture ("Must be handling a user
  // gesture to show a file picker") — that was the bug.
  let importDir = null;
  btn.onclick = async () => {
    const st = statusEl();
    if (!window.showDirectoryPicker) {
      st.innerHTML = `此浏览器不支持网页写入目录（请用 Edge/Chrome）。手动方式：把文件复制到 <code>${esc(rawDir)}/</code>。`;
      return;
    }
    if (!importDir) {
      // Gesture #1: grant the target directory.
      try {
        importDir = await window.showDirectoryPicker({ mode: "readwrite" });
        st.innerHTML = `✅ 已授权写入目录 <code>${esc(importDir.name)}</code>。再点一次「导入资产」预览选要导入的文件（写入到 <code>${esc(rawDir)}/</code>）。`;
      } catch (err) {
        if (err && err.name === "AbortError") { st.textContent = "已取消。"; return; }
        st.innerHTML = `⚠️ ${esc((err && err.message) || String(err))}<br>手动方式：把文件复制到 <code>${esc(rawDir)}/</code>。`;
      }
      return;
    }
    // Gesture #2 (and onward): open the file picker, then write to the handle.
    fi.click();
  };
  fi.onchange = async e => {
    const file = e.target.files[0];
    e.target.value = "";
    const st = statusEl();
    if (!file || !importDir) return;
    try {
      const fh = await importDir.getFileHandle(file.name, { create: true });
      const w = await fh.createWritable();
      await w.write(file);
      await w.close();
      st.innerHTML = `✅ 已写入 <code>${esc(importDir.name)}/${esc(file.name)}</code>（${fmtSize(file.size)}）。请确认它就在 <code>${esc(rawDir)}/</code> 下。` + promptBox(file.name);
    } catch (err) {
      st.innerHTML = `⚠️ ${esc((err && err.message) || String(err))}<br>手动方式：把文件复制到 <code>${esc(rawDir)}/</code>。` + promptBox(file.name);
    }
  };
}
function renderOverview() {
  const lib = DATA.assets.filter(a => a.domain==="library");
  const game = DATA.assets.filter(a => a.domain==="game");
  const it = DATA.intake || [];
  document.getElementById("ov").innerHTML = `
    <h2>AI 助手（你在用的这个）</h2>
    <div class="import-guide">
      本页是静态只读页，<b>不内置聊天框</b>。AI 助手 = 你的编程 agent（工作区助手 / Cursor / Claude Code），它读得到仓库、会调 <code>wash</code>/<code>intake</code> 命令、把结果回报给你。打开它，照下面说即可：
      <table style="margin-top:8px"><tr><th>你想做的事</th><th>对 AI 助手说</th></tr>
      <tr><td>洗白上架</td><td><code>按 SC1 把 raw/&lt;文件名&gt; 洗白入库，目标身高 &lt;米&gt;</code>（洗到 review 停下等你翻图册拍板）</td></tr>
      <tr><td>查资产现状</td><td><code>mushnub 用了哪些动画？谁在引用？</code>（助手读 catalog.json 回答）</td></tr>
      <tr><td>资产体检</td><td><code>跑一遍 SC6 资产体检</code></td></tr></table>
      终端党可跳过 AI，直接一键：<code>art-catalog wash --file &lt;raw路径&gt; --height &lt;米&gt; --license &lt;许可&gt;</code>（详见 README「CLI 契约」）。
      <div style="margin-top:8px"><button class="copybtn" onclick="copyImportGuide(this)">复制洗白触发语模板</button></div>
      <span class="note">场景卡全集：SC1 单件洗白 / SC3 套装拆包 / SC4 动画嫁接 / SC5 换皮接表 / SC6 资产体检（见 tools/art-catalog/scenarios/）。工单状态机：new→washing→review→landed（rejected 旁路），资产表「管线」列实时可见。</span>
    </div>
    <h2>导入新资产</h2>
    <div class="import-guide">
      <div style="margin-bottom:8px"><button class="copybtn" id="import-btn" style="font-size:15px;padding:10px 18px">⬆ 导入资产：选文件 → 写入 raw 目录</button>
      <input type="file" id="import-file" accept=".glb,.gltf,.fbx,.zip,.obj,.png,.jpg,.jpeg,.webp" hidden>
      </div>
      导入成功后页面会给出该文件的触发语与 wash 命令（双轨，一键复制）。
      <div id="import-status" style="margin-top:8px"></div>
    </div>
    <h2>资产健康</h2>
    <table><tr><th>域</th><th>条目</th><th>模型</th><th>被引用</th><th>孤儿</th><th>过时</th></tr>
    <tr><td>通用资产库（${esc(DATA.library||"—")}）</td><td>${lib.length}</td><td>${lib.filter(a=>a.kind==="model").length}</td><td>—</td><td>—</td><td>${lib.filter(a=>a.stale_reasons.length).length}</td></tr>
    <tr><td>游戏资产（${esc(DATA.game)}/assets）</td><td>${game.length}</td><td>${game.filter(a=>a.kind==="model").length}</td><td>${game.filter(a=>a.refs.length).length}</td><td>${game.filter(a=>!a.refs.length&&a.kind!=="other").length}</td><td>${game.filter(a=>a.stale_reasons.length).length}</td></tr></table>
    ${it.length ? `<h2>入库工单</h2><table><tr><th>工单</th><th>状态</th></tr>` +
      it.map(t => `<tr><td><code>${esc(t.file)}</code></td><td>${esc((t.raw&&t.raw.status)||"new")}</td></tr>`).join("") + `</table>` : ""}
    <h2>使用说明</h2><div class="note">本页由 <code>art-catalog</code> 生成（只读扫描）。
    重新生成：<code>cargo run --release -p art-catalog --manifest-path tools/art-catalog/Cargo.toml -- --game games/wave-survival</code>。
    AI 协作接口（catalog.json / report.json / 场景卡契约）见 <code>tools/art-catalog/README.md</code>。</div>`;
  wireImport();
}

renderAssets(); renderGallery(); renderReport(); renderOverview();
</script>
</body>
</html>
"##;
