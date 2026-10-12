# rcmate website

Static website for [rcmate](https://github.com/iocast/rcmate), built with [Zola](https://www.getzola.org) (0.23+).

## Local development

```bash
cd website
zola serve        # http://127.0.0.1:1111
zola build        # output in website/public
```

## Edit content

- Landing page texts, features, quick start: `zola.toml` → `[extra]`
- Docs: `content/docs/*.md`
- Styles (light/dark theme): `static/css/main.css`
- Parallax / theme switch / GitHub stats: `static/js/main.js`

## Deploy to GitHub Pages

1. Repository **Settings → Pages → Build and deployment → Source: GitHub Actions**
2. Push to `main` – `.github/workflows/pages.yml` builds and deploys the site.

The workflow injects the correct `base_url` from GitHub Pages, so custom domains work too.
