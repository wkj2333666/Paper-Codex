import type { Plugin } from "vite"

const VIRTUAL_MODULE = "virtual:paper-codex-pdf-dark-blender"
const RESOLVED_VIRTUAL_MODULE = `\0${VIRTUAL_MODULE}`

export function pdfjsDarkModePlugin(): Plugin {
  return {
    name: "paper-codex-pdfjs-dark-mode",
    enforce: "pre",
    resolveId(id) {
      if (id === VIRTUAL_MODULE) return RESOLVED_VIRTUAL_MODULE
    },
    load(id) {
      if (id === RESOLVED_VIRTUAL_MODULE) return 'export { PdfDarkBlender } from "/src/pdf-dark-blender.ts"'
    },
    transform(source, id) {
      if (!id.split("?", 1)[0].replaceAll("\\", "/").endsWith("/pdfjs-dist/build/pdf.mjs")) return null
      return { code: patchPdfJsDarkMode(source), map: null }
    },
  }
}

export function patchPdfJsDarkMode(source: string) {
  let patched = `import { PdfDarkBlender } from "${VIRTUAL_MODULE}";\n${source}`
  patched = replaceAnchor(patched, `  beginDrawing({
    transform,
    viewport,
    transparency = false,
    background = null
  }) {
    const width = this.ctx.canvas.width;`, `  beginDrawing({
    transform,
    viewport,
    transparency = false,
    background = null
  }) {
    const pdfDarkColors = this.pageColors;
    if (pdfDarkColors) {
      background ||= pdfDarkColors.background;
      this.pageColors = null;
    }
    const width = this.ctx.canvas.width;`, "beginDrawing")

  patched = replaceAnchor(patched, `      this.ctx.transform(...getCurrentTransform(this.compositeCtx));
    }
    this.ctx.save();
    resetCtxToDefault(this.ctx);`, `      this.ctx.transform(...getCurrentTransform(this.compositeCtx));
    }
    if (pdfDarkColors) {
      this.blender = new PdfDarkBlender(this.ctx, pdfDarkColors);
    }
    this.ctx.save();
    resetCtxToDefault(this.ctx);`, "render context")

  patched = replaceAnchor(patched, `    copyCtxState(currentCtx, groupCtx);
    this.ctx = groupCtx;
    this.dependencyTracker?.inheritSimpleDataAsFutureForcedDependencies`, `    copyCtxState(currentCtx, groupCtx);
    this.ctx = groupCtx;
    if (this.blender && !group.smask) {
      this.blender.interceptGroup(this.ctx);
    }
    this.dependencyTracker?.inheritSimpleDataAsFutureForcedDependencies`, "beginGroup")

  patched = replaceAnchor(patched, `    this.groupLevel--;
    const groupCtx = this.ctx;
    const ctx = this.groupStack.pop();`, `    this.groupLevel--;
    const groupCtx = this.ctx;
    if (this.blender) {
      this.blender.cleanupGroup(groupCtx);
    }
    const ctx = this.groupStack.pop();`, "endGroup")

  patched = replaceAnchor(patched, `    this._cachedBitmapsMap.clear();
    this.#drawFilter();
  }
  #drawFilter() {`, `    this._cachedBitmapsMap.clear();
    this.#drawFilter();
    if (this.blender) {
      this.blender.unwrap();
      this.blender = null;
    }
  }
  #drawFilter() {`, "endDrawing")
  return patched
}

function replaceAnchor(source: string, anchor: string, replacement: string, name: string) {
  const first = source.indexOf(anchor)
  if (first < 0 || source.indexOf(anchor, first + anchor.length) >= 0) {
    throw new Error(`PDF.js 5.6.205 dark-mode patch anchor '${name}' did not match exactly once`)
  }
  return source.slice(0, first) + replacement + source.slice(first + anchor.length)
}
