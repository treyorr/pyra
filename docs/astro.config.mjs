// @ts-check
import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";
import mermaid from "astro-mermaid";

const site = process.env.DOCS_SITE_URL || "https://pyra.dev";
const base = process.env.DOCS_BASE_PATH || undefined;

// https://astro.build/config
export default defineConfig({
  site,
  base,
  integrations: [
    mermaid(),
    starlight({
      title: "pyra",
      disable404Route: true,
      logo: {
        src: "./src/assets/icon.png",
      },
      description:
        "A modern Python package and project manager built in Rust.",
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/treyorr/pyra' },
      ],
      head: [],
      customCss: ["./src/styles/custom.css"],
      sidebar: [
        { label: "Introduction", link: "/getting-started/" },
        {
          label: "Concepts",
          autogenerate: { directory: "concepts" },
        },
        {
          label: "Reference",
          autogenerate: { directory: "reference" },
        },
        {
          label: "Internals",
          autogenerate: { directory: "internals" },
        },
        { label: "Roadmap", link: "/roadmap/" },
        { label: "Status", link: "/status/" },
      ],
    }),
  ],
});
