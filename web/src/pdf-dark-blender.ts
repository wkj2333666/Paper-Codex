export type PdfThemeColors = { background: string; foreground: string }

type Rgba = { r: number; g: number; b: number; a: number }
type DrawImage = (...args: unknown[]) => void
type WrappedContext = {
  drawImage: PropertyDescriptor | undefined
  fillStyle: PropertyDescriptor | undefined
  strokeStyle: PropertyDescriptor | undefined
}

const NEUTRAL_DEVIATION = 12
const MAX_SCRATCH_PIXELS = 16_777_216

export function mapPdfColor(style: string, theme: PdfThemeColors) {
  const color = parseColor(style)
  if (!color || !isNeutral(color.r, color.g, color.b, NEUTRAL_DEVIATION)) return style
  const background = parseColor(theme.background)
  const foreground = parseColor(theme.foreground)
  if (!background || !foreground) return style
  const luminance = (0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b) / 255
  return formatColor({
    r: interpolate(foreground.r, background.r, luminance),
    g: interpolate(foreground.g, background.g, luminance),
    b: interpolate(foreground.b, background.b, luminance),
    a: color.a,
  })
}

export function recolorBinaryPdfImage(data: Uint8ClampedArray, theme: PdfThemeColors) {
  const background = parseColor(theme.background)
  const foreground = parseColor(theme.foreground)
  if (!background || !foreground || data.length < 8) return false

  const pixelCount = data.length >>> 2
  const step = Math.max(1, Math.floor(pixelCount / 4096))
  let visible = 0
  let transparent = 0
  let neutral = 0
  let endpoint = 0
  let dark = 0
  let light = 0
  for (let pixel = 0; pixel < pixelCount; pixel += step) {
    const offset = pixel * 4
    if (data[offset + 3] < 16) {
      transparent += 1
      continue
    }
    visible += 1
    const r = data[offset]
    const g = data[offset + 1]
    const b = data[offset + 2]
    if (!isNeutral(r, g, b, NEUTRAL_DEVIATION)) continue
    neutral += 1
    const luminance = (r + g + b) / 3
    if (luminance <= 64) {
      endpoint += 1
      dark += 1
    } else if (luminance >= 191) {
      endpoint += 1
      light += 1
    }
  }

  const hasBackgroundEndpoint = light > 0 || transparent > 0
  if (visible < 2 || neutral !== visible || endpoint / neutral < 0.85 || dark === 0 || !hasBackgroundEndpoint) return false

  for (let offset = 0; offset < data.length; offset += 4) {
    const r = data[offset]
    const g = data[offset + 1]
    const b = data[offset + 2]
    if (!isNeutral(r, g, b, NEUTRAL_DEVIATION)) continue
    const luminance = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255
    data[offset] = interpolate(foreground.r, background.r, luminance)
    data[offset + 1] = interpolate(foreground.g, background.g, luminance)
    data[offset + 2] = interpolate(foreground.b, background.b, luminance)
  }
  return true
}

/**
 * Inline canvas interception follows Zotero Reader's PDF.js Blender design,
 * which was adapted from doq. Image opacity and chromatic pixels are retained.
 */
export class PdfDarkBlender {
  private readonly wrapped = new Map<CanvasRenderingContext2D, WrappedContext>()
  private readonly renderedGroupCanvases = new WeakSet<object>()

  constructor(private readonly context: CanvasRenderingContext2D, private readonly theme: PdfThemeColors) {
    this.wrap(context)
  }

  interceptGroup(context: CanvasRenderingContext2D) {
    if (typeof context.canvas === "object" && context.canvas) this.renderedGroupCanvases.add(context.canvas)
    this.wrap(context)
  }

  cleanupGroup(context: CanvasRenderingContext2D) {
    this.restore(context)
  }

  unwrap() {
    for (const context of [...this.wrapped.keys()]) this.restore(context)
  }

  private wrap(context: CanvasRenderingContext2D) {
    if (this.wrapped.has(context)) return
    const saved: WrappedContext = {
      drawImage: Object.getOwnPropertyDescriptor(context, "drawImage"),
      fillStyle: Object.getOwnPropertyDescriptor(context, "fillStyle"),
      strokeStyle: Object.getOwnPropertyDescriptor(context, "strokeStyle"),
    }
    this.wrapped.set(context, saved)
    this.wrapStyle(context, "fillStyle")
    this.wrapStyle(context, "strokeStyle")
    const originalDrawImage = context.drawImage.bind(context) as DrawImage
    Object.defineProperty(context, "drawImage", {
      configurable: true,
      writable: true,
      value: (...args: unknown[]) => this.drawImage(context, originalDrawImage, args),
    })
  }

  private wrapStyle(context: CanvasRenderingContext2D, property: "fillStyle" | "strokeStyle") {
    const descriptor = inheritedDescriptor(context, property)
    if (!descriptor?.get || !descriptor.set) return
    Object.defineProperty(context, property, {
      configurable: true,
      enumerable: descriptor.enumerable,
      get: () => descriptor.get?.call(context),
      set: value => {
        descriptor.set?.call(context, value)
        const normalized = descriptor.get?.call(context)
        if (typeof normalized === "string") descriptor.set?.call(context, mapPdfColor(normalized, this.theme))
      },
    })
  }

  private restore(context: CanvasRenderingContext2D) {
    const saved = this.wrapped.get(context)
    if (!saved) return
    restoreOwnDescriptor(context, "drawImage", saved.drawImage)
    restoreOwnDescriptor(context, "fillStyle", saved.fillStyle)
    restoreOwnDescriptor(context, "strokeStyle", saved.strokeStyle)
    this.wrapped.delete(context)
  }

  private drawImage(context: CanvasRenderingContext2D, original: DrawImage, args: unknown[]) {
    const source = args[0]
    if (!source || (typeof source === "object" && this.renderedGroupCanvases.has(source))) {
      original(...args)
      return
    }

    const geometry = imageGeometry(source, args)
    if (!geometry) {
      original(...args)
      return
    }
    const sampleScale = Math.min(1, 64 / Math.max(geometry.sourceWidth, geometry.sourceHeight))
    const sample = createScratchCanvas(context, geometry.sourceWidth * sampleScale, geometry.sourceHeight * sampleScale)
    const sampleContext = sample?.getContext("2d", { willReadFrequently: true })
    if (!sample || !sampleContext) {
      original(...args)
      return
    }
    try {
      sampleContext.imageSmoothingEnabled = false
      sampleContext.drawImage(source as CanvasImageSource, geometry.sx, geometry.sy, geometry.sourceWidth, geometry.sourceHeight, 0, 0, sample.width, sample.height)
      const sampleData = sampleContext.getImageData(0, 0, sample.width, sample.height)
      if (!recolorBinaryPdfImage(sampleData.data, this.theme)) {
        original(...args)
        return
      }
    } catch {
      original(...args)
      return
    } finally {
      sample.width = 0
      sample.height = 0
    }

    const fullScale = Math.min(1, Math.sqrt(MAX_SCRATCH_PIXELS / (geometry.sourceWidth * geometry.sourceHeight)))
    const scratch = createScratchCanvas(context, geometry.sourceWidth * fullScale, geometry.sourceHeight * fullScale)
    const scratchContext = scratch?.getContext("2d", { willReadFrequently: true })
    if (!scratch || !scratchContext) {
      original(...args)
      return
    }

    try {
      scratchContext.drawImage(source as CanvasImageSource, geometry.sx, geometry.sy, geometry.sourceWidth, geometry.sourceHeight, 0, 0, scratch.width, scratch.height)
      const imageData = scratchContext.getImageData(0, 0, scratch.width, scratch.height)
      if (!recolorBinaryPdfImage(imageData.data, this.theme)) {
        original(...args)
        return
      }
      scratchContext.putImageData(imageData, 0, 0)
      original(scratch, 0, 0, scratch.width, scratch.height, geometry.dx, geometry.dy, geometry.drawWidth, geometry.drawHeight)
    } catch {
      original(...args)
    } finally {
      scratch.width = 0
      scratch.height = 0
    }
  }
}

function imageGeometry(source: unknown, args: unknown[]) {
  const image = source as { width?: number; height?: number; naturalWidth?: number; naturalHeight?: number; displayWidth?: number; displayHeight?: number }
  const width = image.naturalWidth ?? image.displayWidth ?? image.width
  const height = image.naturalHeight ?? image.displayHeight ?? image.height
  if (typeof width !== "number" || typeof height !== "number" || !Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) return null
  if (args.length === 3) {
    return { sx: 0, sy: 0, sourceWidth: width, sourceHeight: height, dx: numberArg(args[1]), dy: numberArg(args[2]), drawWidth: width, drawHeight: height }
  }
  if (args.length === 5) {
    return { sx: 0, sy: 0, sourceWidth: width, sourceHeight: height, dx: numberArg(args[1]), dy: numberArg(args[2]), drawWidth: numberArg(args[3]), drawHeight: numberArg(args[4]) }
  }
  if (args.length === 9) {
    const sourceWidth = numberArg(args[3])
    const sourceHeight = numberArg(args[4])
    if (sourceWidth <= 0 || sourceHeight <= 0) return null
    return { sx: numberArg(args[1]), sy: numberArg(args[2]), sourceWidth, sourceHeight, dx: numberArg(args[5]), dy: numberArg(args[6]), drawWidth: numberArg(args[7]), drawHeight: numberArg(args[8]) }
  }
  return null
}

function createScratchCanvas(context: CanvasRenderingContext2D, width: number, height: number) {
  const pixelWidth = Math.max(1, Math.ceil(width))
  const pixelHeight = Math.max(1, Math.ceil(height))
  const ownerDocument = (context.canvas as HTMLCanvasElement).ownerDocument ?? globalThis.document
  if (ownerDocument) {
    const canvas = ownerDocument.createElement("canvas")
    canvas.width = pixelWidth
    canvas.height = pixelHeight
    return canvas
  }
  if (typeof OffscreenCanvas !== "undefined") return new OffscreenCanvas(pixelWidth, pixelHeight)
  return null
}

function inheritedDescriptor(value: object, property: string) {
  let current = Object.getPrototypeOf(value)
  while (current) {
    const descriptor = Object.getOwnPropertyDescriptor(current, property)
    if (descriptor) return descriptor
    current = Object.getPrototypeOf(current)
  }
  return undefined
}

function restoreOwnDescriptor(context: object, property: string, descriptor: PropertyDescriptor | undefined) {
  if (descriptor) Object.defineProperty(context, property, descriptor)
  else Reflect.deleteProperty(context, property)
}

function numberArg(value: unknown) {
  return typeof value === "number" ? value : 0
}

function isNeutral(r: number, g: number, b: number, deviation: number) {
  return Math.max(r, g, b) - Math.min(r, g, b) <= deviation
}

function interpolate(start: number, end: number, amount: number) {
  return Math.round(start + (end - start) * amount)
}

function formatColor(color: Rgba) {
  if (color.a < 1) return `rgba(${color.r}, ${color.g}, ${color.b}, ${color.a})`
  return `#${[color.r, color.g, color.b].map(component => component.toString(16).padStart(2, "0")).join("")}`
}

function parseColor(value: string): Rgba | null {
  const hex = value.match(/^#([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i)?.[1]
  if (hex) {
    const expanded = hex.length === 3 ? [...hex].map(value => value + value).join("") : hex
    return {
      r: Number.parseInt(expanded.slice(0, 2), 16),
      g: Number.parseInt(expanded.slice(2, 4), 16),
      b: Number.parseInt(expanded.slice(4, 6), 16),
      a: expanded.length === 8 ? Number.parseInt(expanded.slice(6, 8), 16) / 255 : 1,
    }
  }
  const rgb = value.match(/^rgba?\(\s*([\d.]+)[, ]+([\d.]+)[, ]+([\d.]+)(?:\s*[,/]\s*([\d.]+))?\s*\)$/i)
  if (!rgb) return null
  return { r: Number(rgb[1]), g: Number(rgb[2]), b: Number(rgb[3]), a: rgb[4] === undefined ? 1 : Number(rgb[4]) }
}
