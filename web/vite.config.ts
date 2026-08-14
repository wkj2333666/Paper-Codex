import { defineConfig } from "vite"
import react from "@vitejs/plugin-react"
import { pdfjsDarkModePlugin } from "./pdfjs-dark-mode-plugin"
export default defineConfig({cacheDir:"../.cache/vite",plugins:[pdfjsDarkModePlugin(),react()],optimizeDeps:{exclude:["pdfjs-dist"]},server:{host:"127.0.0.1",port:5173,proxy:{"/api":"http://127.0.0.1:3000"}},build:{outDir:"dist",sourcemap:false},test:{environment:"node"}})
