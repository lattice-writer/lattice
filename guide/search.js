(() => {
  const input = document.getElementById("search");
  const list = document.getElementById("results");
  const index = window.LATTICE_GUIDE_INDEX || [];
  let active = -1;
  const lower = (text) => text.toLocaleLowerCase();
  const snippet = (text, term) => {
    const from = Math.max(0, lower(text).indexOf(term) - 50);
    return (from > 0 ? "…" : "") + text.slice(from, from + 160) + (from + 160 < text.length ? "…" : "");
  };
  const mark = (parent, text, terms) => {
    const haystack = lower(text);
    let from = 0;
    while (from < text.length) {
      let at = -1;
      let length = 0;
      for (const term of terms) {
        const found = haystack.indexOf(term, from);
        if (found >= 0 && (at < 0 || found < at)) [at, length] = [found, term.length];
      }
      if (at < 0) break;
      parent.append(text.slice(from, at));
      const highlight = document.createElement("mark");
      highlight.textContent = text.slice(at, at + length);
      parent.append(highlight);
      from = at + length;
    }
    parent.append(text.slice(from));
  };
  const select = (next) => {
    const items = list.querySelectorAll("a");
    if (!items.length) return;
    active = (next + items.length) % items.length;
    items.forEach((item, i) => item.toggleAttribute("aria-selected", i === active));
    items[active].scrollIntoView({ block: "nearest" });
  };
  const show = () => {
    const terms = lower(input.value).split(/\s+/).filter(Boolean);
    list.replaceChildren();
    active = -1;
    if (!terms.length) {
      list.hidden = true;
      return;
    }
    const hits = [];
    for (const entry of index) {
      const title = lower(entry.title);
      const text = lower(entry.text);
      let score = 0;
      for (const term of terms) {
        if (title.includes(term)) score += 10;
        else if (text.includes(term)) score += 1;
        else {
          score = 0;
          break;
        }
      }
      if (score) hits.push([score, entry]);
    }
    hits.sort((a, b) => b[0] - a[0]);
    for (const [, entry] of hits.slice(0, 12)) {
      const item = document.createElement("li");
      const link = document.createElement("a");
      link.href = entry.href;
      const title = document.createElement("strong");
      mark(title, entry.title, terms);
      const text = document.createElement("span");
      mark(text, snippet(entry.text, terms[0]), terms);
      link.append(title, text);
      item.append(link);
      list.append(item);
    }
    if (!hits.length) {
      const item = document.createElement("li");
      item.className = "empty";
      item.textContent = input.dataset.empty;
      list.append(item);
    }
    list.hidden = false;
  };
  input.addEventListener("input", show);
  input.addEventListener("focus", () => input.value && show());
  input.addEventListener("keydown", (event) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      select(active + 1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      select(active - 1);
    } else if (event.key === "Enter") {
      const target = list.querySelectorAll("a")[Math.max(active, 0)];
      if (target) location.href = target.href;
    } else if (event.key === "Escape") {
      input.value = "";
      show();
      input.blur();
    }
  });
  document.addEventListener("keydown", (event) => {
    const typing = /^(INPUT|TEXTAREA)$/.test(document.activeElement?.tagName ?? "");
    if ((event.key === "/" && !typing) || (event.key === "k" && (event.metaKey || event.ctrlKey))) {
      event.preventDefault();
      input.focus();
      input.select();
    }
  });
  document.addEventListener("click", (event) => {
    if (!event.target.closest(".search")) list.hidden = true;
  });
})();
