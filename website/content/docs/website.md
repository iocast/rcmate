+++
title = "About this website"
description = "How this site is built with Zola and deployed to GitHub Pages."
weight = 7
+++

This website is a static site generated with [Zola](https://www.getzola.org) and deployed to GitHub Pages with GitHub Actions.

## Run it locally

```bash
cd website
zola serve
```

Open <http://127.0.0.1:1111>. Changes reload automatically.

## Structure

| Path | Purpose |
|------|---------|
| `website/zola.toml` | Site config, landing page texts, features and quick start steps |
| `website/content/` | Markdown pages, e.g. this documentation |
| `website/templates/` | Tera templates for layout |
| `website/static/css/main.css` | Styles, light and dark theme |
| `website/static/js/` | Theme switch, parallax, GitHub stats |
| `.github/workflows/pages.yml` | Build and deploy to GitHub Pages |

## Deployment

Every push to `main` that touches `website/` builds the site and publishes it. In the repository settings, set **Pages → Build and deployment → Source** to **GitHub Actions**.
