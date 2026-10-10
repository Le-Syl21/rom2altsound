// The sound ROM catalog: reads catalog.json (written by `rom2altsound catalog`) and shows
// it as a searchable, sortable table, with one detail panel per entry (#<sound ROM id> or
// #set:<name>). No framework, no external host.
(function () {
  "use strict";
  var lang = document.documentElement.lang === "fr" ? "fr" : "en";
  var T = {
    en: {
      loading: "Loading the catalog…",
      failed: "The catalog could not be loaded (catalog.json). Opened from a file? Serve the folder instead, for example: python3 -m http.server -d docs",
      entries: "entries", ids: "sound ROM ids", sets: "PinMAME sets", found: "sets in the reference set",
      works: "Works", partial: "Partial", none: "No sound yet", "no-board": "No sound board", untested: "Not tried",
      all: "All", shown: function (n, m, t) { return (n < m ? fmt(n) + " shown of " : "") + fmt(m) + " matching, of " + fmt(t) + " entries"; },
      more: "Show more", game: "Game", maker: "Maker", year: "Year", family: "Board family",
      support: "rom2altsound", setsCol: "Sets", sounds: "Sounds", names: "Names", id: "Sound ROM id",
      close: "Close", copy: "Copy link", copied: "Copied",
      dIdNone: "None: the sounds are not in sound ROMs of their own",
      dFamily: "Sound board family", dSupport: "rom2altsound support", dSounds: "Read from the ROMs",
      dNames: "Commands named in sounds.dat", dDoc: "Family notes",
      tracks: function (s) { return s.count + " DCS tracks (" + s.slots + " catalog slots)"; },
      calls: function (s) { return s.count + " sound calls, " + s.samples + " samples" + (s.music ? ", " + s.music + " music scripts" : ""); },
      notRead: "not read statically for this board",
      hSets: "Sets sharing these sound ROMs", hSet: "Set", hRoms: "Sound ROM files", hSam: "Image the sounds are read from",
      sName: "Set", sParent: "Parent", sDesc: "Description", sFound: "Reference set", file: "File", size: "Size",
      f: { good: "good", split: "split (with parent)", completable: "completable", "bad-dump": "bad dump", incomplete: "incomplete", absent: "not there" },
      none_roms: "No sound ROM: the board makes its sounds without one (tones, chimes) or the game has no sound board.",
      onlyIds: "Only sound ROM ids", onlyFound: "Only in the reference set", search: "Search: game, set, sound ROM id",
      notFound: "No entry for this link.",
      bySupport: "PinMAME sets by rom2altsound support", dash: "—",
      boards: { "NONE (Stern SAM)": "Stern SAM", "NONE (other)": "No sound board" },
      noVpx: "AltSound pack not played in VPX", newVpx: function (v) { return "pack plays in VPX after " + v; }, hideNoVpx: "Hide boards whose AltSound pack VPinball cannot play yet", dVpx: "AltSound pack in VPinball",
    },
    fr: {
      loading: "Chargement du catalogue…",
      failed: "Impossible de charger le catalogue (catalog.json). Page ouverte depuis un fichier ? Servez le dossier, par exemple : python3 -m http.server -d docs",
      entries: "entrées", ids: "identifiants de ROM son", sets: "jeux de ROM PinMAME", found: "jeux dans la collection de référence",
      works: "Fonctionne", partial: "Partiel", none: "Pas encore de son", "no-board": "Pas de carte son", untested: "Non essayé",
      all: "Tous", shown: function (n, m, t) { return (n < m ? fmt(n) + " affichées sur " : "") + fmt(m) + " trouvées, sur " + fmt(t) + " entrées"; },
      more: "Afficher plus", game: "Jeu", maker: "Fabricant", year: "Année", family: "Famille de carte",
      support: "rom2altsound", setsCol: "Jeux de ROM", sounds: "Sons", names: "Noms", id: "Id de ROM son",
      close: "Fermer", copy: "Copier le lien", copied: "Copié",
      dIdNone: "Aucun : les sons ne sont pas dans des ROM son à part",
      dFamily: "Famille de carte son", dSupport: "Prise en charge par rom2altsound", dSounds: "Lu dans les ROM",
      dNames: "Commandes nommées dans sounds.dat", dDoc: "Notes sur la famille",
      tracks: function (s) { return s.count + " pistes DCS (" + s.slots + " emplacements au catalogue)"; },
      calls: function (s) { return s.count + " appels de son, " + s.samples + " échantillons" + (s.music ? ", " + s.music + " scripts de musique" : ""); },
      notRead: "non lu sans émulation pour cette carte",
      hSets: "Jeux de ROM qui partagent ces ROM son", hSet: "Jeu de ROM", hRoms: "Fichiers des ROM son", hSam: "Image d'où les sons sont lus",
      sName: "Jeu de ROM", sParent: "Parent", sDesc: "Description", sFound: "Collection de référence", file: "Fichier", size: "Taille",
      f: { good: "bon", split: "séparé (avec le parent)", completable: "complétable", "bad-dump": "mauvais dump", incomplete: "incomplet", absent: "absent" },
      none_roms: "Pas de ROM son : la carte fait ses sons sans (tonalités, carillons) ou le jeu n'a pas de carte son.",
      onlyIds: "Seulement les ids de ROM son", onlyFound: "Seulement la collection de référence", search: "Recherche : jeu, jeu de ROM, id de ROM son",
      notFound: "Aucune entrée pour ce lien.",
      bySupport: "Jeux de ROM PinMAME par prise en charge", dash: "—",
      boards: { "NONE (Stern SAM)": "Stern SAM", "NONE (other)": "Aucune carte son" },
      noVpx: "pack AltSound non joué dans VPX", newVpx: function (v) { return "pack joué par VPX après " + v; }, hideNoVpx: "Masquer les cartes dont VPinball ne joue pas encore le pack AltSound", dVpx: "Pack AltSound dans VPinball",
    },
  }[lang];
  var SUPPORT_ORDER = ["works", "partial", "none", "no-board", "untested"];
  var PAGE = 200;

  var $ = function (id) { return document.getElementById(id); };
  function esc(s) {
    return String(s == null ? "" : s).replace(/[&<>"']/g, function (c) {
      return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c];
    });
  }
  function fmt(n) { return Number(n).toLocaleString(lang === "fr" ? "fr-FR" : "en-GB"); }
  function bytes(n) {
    if (n >= 1048576) return (n / 1048576).toFixed(n % 1048576 ? 2 : 0) + " MiB";
    if (n >= 1024) return (n / 1024).toFixed(n % 1024 ? 1 : 0) + " KiB";
    return n + " B";
  }
  function short(f) { return f.replace(/SNDBRD_/g, ""); }
  // The board as shown: PinMAME's "NONE (...)" labels get a plain name (the label itself
  // stays the filter value and the tooltip).
  function boardName(label) { return T.boards[label] || short(label); }
  function dash() { return '<span class="sets">' + esc(T.dash) + "</span>"; }
  // Why the entry's packs cannot play in VPinball (vpx_playable false), or play only in
  // its builds newer than its last release ("newer"), from vpx_playback.json through
  // build_site.py; "" when nothing is known against it.
  function vpxNote(e) { return e.vpx_playable === false || e.vpx_playable === "newer"; }
  function vpxReason(e) {
    if (!vpxNote(e)) return "";
    var r = (data.vpx_playback || {})[e.label];
    return r ? r[lang] || r.en : "";
  }
  function vpxBadge(e) {
    if (!vpxNote(e)) return "";
    var newer = e.vpx_playable === "newer";
    return ' <span class="badge ' + (newer ? "b-newvpx" : "b-novpx") + '" title="' + esc(vpxReason(e)) + '">' +
      esc(newer ? T.newVpx(data.vpx_newer_than || "") : T.noVpx) + "</span>";
  }
  function badge(s) { return '<span class="badge b-' + esc(s) + '">' + esc(T[s] || s) + "</span>"; }
  function soundCount(e) { return e.sounds ? e.sounds.count : -1; }
  function foundCount(e) { return e.sets.filter(function (s) { return s.found !== "absent"; }).length; }

  var data, entries = [], view = [], shown = PAGE, sortKey = "title", sortDir = 1;
  var byKey = {};

  // The family's notes: one link per board of the family ("SNDBRD_S11XS+SNDBRD_S11CS"),
  // the board survey when the per-family docs do not list it.
  function docLink(family, label) {
    var docs = data.family_docs || {};
    // A link for the whole label first: Stern SAM's "no board" apart from the others'
    // ("SNDBRD_NONE (other)"), a two-board family's own section ("SNDBRD_S11XS+SNDBRD_S11CS").
    var name = boardName(label);
    var whole = docs["SNDBRD_" + label] || docs[family];
    if (whole) return '<a href="' + esc(whole) + '"' + (name !== short(label) ? ' title="' + esc(label) + '"' : "") + ">" + esc(name) + "</a>";
    var parts = family.split("+"), out = [];
    parts.forEach(function (p) {
      var u = docs[p];
      if (u) out.push('<a href="' + esc(u) + '">' + esc(parts.length > 1 ? short(p) : name) + "</a>");
    });
    if (out.length !== parts.length) return '<a href="' + esc(data.family_docs_default) + '">' + esc(name) + "</a>";
    return out.join(" + ");
  }

  function haystack(e) {
    var parts = [e.title, e.manufacturer, e.key, e.label];
    e.sets.forEach(function (s) { parts.push(s.name, s.description, s.manufacturer); });
    return parts.join("\u0001").toLowerCase();
  }

  function fillSelect(sel, values, labels) {
    values.forEach(function (v) {
      var o = document.createElement("option");
      o.value = v;
      o.textContent = labels ? labels(v) : v;
      sel.appendChild(o);
    });
  }

  function stats() {
    var s = data.stats;
    var cards = [
      [s.entries, T.entries], [s.sound_rom_ids, T.ids], [s.sets, T.sets], [s.sets_found, T.found],
    ];
    // The support counts share one card, so the row does not leave one of them alone.
    var subs = SUPPORT_ORDER.filter(function (k) { return s.support[k]; }).map(function (k) {
      return '<div class="sub"><b>' + fmt(s.support[k].sets) + "</b>" + badge(k) + "</div>";
    });
    $("stats").innerHTML = cards.map(function (c) {
      return '<div class="stat"><b>' + fmt(c[0]) + "</b><span>" + esc(c[1]) + "</span></div>";
    }).join("") +
      '<div class="stat group"><span>' + esc(T.bySupport) + '</span><div class="subs">' + subs.join("") + "</div></div>";
  }

  function compare(a, b) {
    var x, y;
    switch (sortKey) {
      case "maker": x = a.manufacturer.toLowerCase(); y = b.manufacturer.toLowerCase(); break;
      case "year": x = a.year; y = b.year; break;
      case "family": x = boardName(a.label).toLowerCase(); y = boardName(b.label).toLowerCase(); break;
      case "support": x = SUPPORT_ORDER.indexOf(a.support); y = SUPPORT_ORDER.indexOf(b.support); break;
      case "sets": x = a.sets.length; y = b.sets.length; break;
      case "sounds": x = soundCount(a); y = soundCount(b); break;
      case "names": x = a.names; y = b.names; break;
      case "id": x = a.id || "~"; y = b.id || "~"; break;
      default: x = a.title.toLowerCase(); y = b.title.toLowerCase();
    }
    if (x < y) return -sortDir;
    if (x > y) return sortDir;
    return a.title.toLowerCase() < b.title.toLowerCase() ? -1 : a.title.toLowerCase() > b.title.toLowerCase() ? 1 : (a.key < b.key ? -1 : 1);
  }

  function apply() {
    var q = $("q").value.trim().toLowerCase();
    var fam = $("family").value, sup = $("support").value, maker = $("maker").value;
    var onlyIds = $("onlyIds").checked, onlyFound = $("onlyFound").checked, hideNoVpx = $("hideNoVpx").checked;
    var words = q.split(/\s+/).filter(Boolean);
    view = entries.filter(function (e) {
      if (fam && e.label !== fam) return false;
      if (sup && e.support !== sup) return false;
      if (maker && !e._makers[maker]) return false;
      if (onlyIds && !e.id) return false;
      if (onlyFound && !foundCount(e)) return false;
      if (hideNoVpx && e.vpx_playable === false) return false;
      for (var i = 0; i < words.length; i++) if (e._hay.indexOf(words[i]) < 0) return false;
      return true;
    });
    view.sort(compare);
    shown = PAGE;
    render();
    try {
      var u = new URL(location.href);
      if (q) u.searchParams.set("q", q); else u.searchParams.delete("q");
      history.replaceState(null, "", u);
    } catch (err) { /* file: URLs */ }
  }

  function row(e) {
    var found = foundCount(e);
    var sets = e.sets.slice(0, 4).map(function (s) { return s.name; }).join(", ") + (e.sets.length > 4 ? ", …" : "");
    var snd = e.sounds ? fmt(e.sounds.count) : "";
    var board = boardName(e.label);
    return "<tr>" +
      '<td><a class="game" href="#' + esc(e.key) + '">' + esc(e.title) + '</a><div class="sets">' + esc(sets) + "</div></td>" +
      '<td class="hide-sm">' + esc(e.manufacturer) + "</td>" +
      "<td>" + esc(e.year) + "</td>" +
      "<td" + (board !== short(e.label) ? ' title="' + esc(e.label) + '"' : "") + ">" + esc(board) + "</td>" +
      "<td>" + badge(e.support) + vpxBadge(e) + "</td>" +
      '<td class="num">' + found + " / " + e.sets.length + "</td>" +
      '<td class="num">' + snd + "</td>" +
      '<td class="num">' + (e.names ? fmt(e.names) : dash()) + "</td>" +
      '<td class="hide-sm">' + (e.id ? '<code class="id" title="' + esc(e.id) + '">' + esc(e.id.slice(0, 10)) + "</code>" : '<span class="sets" title="' + esc(T.dIdNone) + '">' + esc(T.dash) + "</span>") + "</td>" +
      "</tr>";
  }

  function render() {
    $("rows").innerHTML = view.slice(0, shown).map(row).join("");
    var n = Math.min(shown, view.length);
    $("count").textContent = T.shown(n, view.length, entries.length);
    $("more").hidden = view.length <= shown;
    document.querySelectorAll("th[data-k]").forEach(function (th) {
      th.setAttribute("aria-sort", th.dataset.k === sortKey ? (sortDir > 0 ? "ascending" : "descending") : "none");
    });
  }

  function detail() {
    var key = decodeURIComponent(location.hash.slice(1));
    var box = $("detail");
    if (!key || key === "about" || key === "catalog") { box.hidden = true; return; }
    var e = byKey[key];
    if (!e && /^[0-9a-f]{4,40}$/.test(key)) {
      var m = entries.filter(function (x) { return x.id && x.id.indexOf(key) === 0; });
      if (m.length === 1) e = m[0];
    }
    if (!e) {
      if (document.getElementById(key)) { box.hidden = true; return; }
      box.innerHTML = '<p>' + esc(T.notFound) + "</p>";
      box.hidden = false;
      return;
    }
    var s = e.sounds;
    var soundsTxt = s ? (s.kind === "dcs-tracks" ? T.tracks(s) : T.calls(s)) : T.notRead;
    var h = '<button class="btn close" type="button" id="close">' + esc(T.close) + "</button>" +
      "<h2>" + esc(e.title) + "</h2>" +
      '<p class="sets">' + esc(e.manufacturer) + " · " + esc(e.year) + "</p>" +
      "<dl>" +
      "<dt>" + esc(T.id) + "</dt><dd>" + (e.id ? "<code>" + esc(e.id) + "</code>" : esc(T.dIdNone)) + "</dd>" +
      "<dt>" + esc(T.dFamily) + "</dt><dd>" + docLink(e.family, e.label) + "</dd>" +
      "<dt>" + esc(T.dSupport) + "</dt><dd>" + badge(e.support) + "</dd>" +
      (vpxNote(e) ? "<dt>" + esc(T.dVpx) + "</dt><dd>" + vpxBadge(e).trim() + " " + esc(vpxReason(e)) + "</dd>" : "") +
      "<dt>" + esc(T.dSounds) + "</dt><dd>" + esc(soundsTxt) + "</dd>" +
      "<dt>" + esc(T.dNames) + "</dt><dd>" + fmt(e.names) + "</dd>" +
      "</dl>" +
      '<button class="btn" type="button" id="copy">' + esc(T.copy) + "</button>" +
      "<h3>" + esc(e.id ? T.hSets : T.hSet) + "</h3>" +
      '<div class="table"><table><thead><tr><th>' + esc(T.sName) + "</th><th>" + esc(T.sDesc) + '</th><th class="hide-sm">' + esc(T.maker) + "</th><th>" + esc(T.year) + "</th><th>" + esc(T.sFound) + '</th><th class="num">' + esc(T.names) + "</th></tr></thead><tbody>" +
      e.sets.map(function (x) {
        return "<tr><td><code>" + esc(x.name) + "</code>" + (x.parent ? '<div class="sets">' + esc(T.sParent) + " " + esc(x.parent) + "</div>" : "") + "</td><td>" + esc(x.description) +
          '</td><td class="hide-sm">' + esc(x.manufacturer) + "</td><td>" + esc(x.year) + "</td><td>" + esc(T.f[x.found] || x.found) + '</td><td class="num">' + (x.names ? fmt(x.names) : dash()) + "</td></tr>";
      }).join("") + "</tbody></table></div>" +
      "<h3>" + esc(e.id ? T.hRoms : T.hSam) + "</h3>" +
      (e.roms.length
        ? '<div class="table"><table><thead><tr><th>' + esc(T.file) + '</th><th class="num">' + esc(T.size) + "</th><th>CRC32</th><th>SHA-1</th></tr></thead><tbody>" +
          e.roms.map(function (r) {
            return "<tr><td><code>" + esc(r.name) + '</code></td><td class="num">' + esc(bytes(r.size)) + "</td><td><code>" + esc(r.crc) + '</code></td><td><code class="id">' + esc(r.sha1 || "") + "</code></td></tr>";
          }).join("") + "</tbody></table></div>"
        : "<p>" + esc(T.none_roms) + "</p>");
    box.innerHTML = h;
    box.hidden = false;
    $("close").onclick = function () { history.pushState(null, "", location.pathname + location.search); detail(); };
    $("copy").onclick = function () {
      var b = this;
      if (navigator.clipboard) navigator.clipboard.writeText(location.href).then(function () { b.textContent = T.copied; });
    };
    box.scrollIntoView({ block: "start" });
    document.title = e.title + " · " + document.querySelector("meta[property='og:site_name']").content;
  }

  function init(json) {
    data = json;
    entries = data.entries;
    var makers = {}, labels = {};
    entries.forEach(function (e) {
      e._hay = haystack(e);
      e._makers = {};
      e.sets.forEach(function (s) { if (s.manufacturer) { e._makers[s.manufacturer] = 1; } });
      Object.keys(e._makers).forEach(function (m) { makers[m] = (makers[m] || 0) + e.sets.length; });
      labels[e.label] = 1;
      byKey[e.key] = e;
    });
    stats();
    $("hideNoVpx").parentNode.hidden = !entries.some(function (e) { return e.vpx_playable === false; });
    fillSelect($("family"), Object.keys(labels).sort(function (a, b) {
      var x = boardName(a).toLowerCase(), y = boardName(b).toLowerCase();
      return x < y ? -1 : x > y ? 1 : 0;
    }), boardName);
    fillSelect($("support"), SUPPORT_ORDER.filter(function (k) { return data.stats.support[k]; }), function (k) { return T[k]; });
    fillSelect($("maker"), Object.keys(makers).sort(function (a, b) { return makers[b] - makers[a] || (a < b ? -1 : 1); }));
    try { $("q").value = new URL(location.href).searchParams.get("q") || ""; } catch (err) { /* file: URLs */ }
    ["q", "family", "support", "maker", "onlyIds", "onlyFound", "hideNoVpx"].forEach(function (id) {
      $(id).addEventListener(id === "q" ? "input" : "change", apply);
    });
    document.querySelectorAll("th[data-k] button").forEach(function (b) {
      b.addEventListener("click", function () {
        var k = b.parentNode.dataset.k;
        if (k === sortKey) sortDir = -sortDir; else { sortKey = k; sortDir = k === "title" || k === "maker" || k === "family" || k === "year" || k === "support" || k === "id" ? 1 : -1; }
        view.sort(compare);
        render();
      });
    });
    $("more").addEventListener("click", function () { shown += PAGE * 2; render(); });
    window.addEventListener("hashchange", detail);
    $("status").hidden = true;
    $("app").hidden = false;
    apply();
    detail();
  }

  // Static labels of the controls.
  $("q").placeholder = T.search;
  $("lOnlyIds").textContent = T.onlyIds;
  $("lOnlyFound").textContent = T.onlyFound;
  $("lHideNoVpx").textContent = T.hideNoVpx;
  document.querySelectorAll("select option[value='']").forEach(function (o) { o.textContent = T.all; });
  $("more").textContent = T.more;
  $("status").textContent = T.loading;

  var base = document.documentElement.dataset.base || "";
  fetch(base + "catalog.json")
    .then(function (r) { if (!r.ok) throw new Error(r.status); return r.json(); })
    .then(init)
    .catch(function () { $("status").textContent = T.failed; });
})();
