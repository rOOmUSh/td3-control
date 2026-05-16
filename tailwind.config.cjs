module.exports = {
  darkMode: "class",
  content: [
    "./ui/**/*.html",
    "./ui/**/*.js",
  ],
  theme: {
    extend: {
      colors: {
        "surface-container-lowest": "#0d0f0f",
        "surface-container-low": "#1a1c1c",
        "surface-container": "#1e2020",
        "surface-container-high": "#282a2a",
        "surface-container-highest": "#333535",
        "surface-dim": "#121414",
        "surface-bright": "#383939",
        "surface": "#121414",
        "on-surface": "#e2e2e2",
        "on-surface-variant": "#b8c4b8",
        "primary-fixed": "#4e8c45",
        "primary-fixed-dim": "#3a6a33",
        "primary-container": "#2d5a27",
        "on-primary-container": "#d1ffd1",
        "secondary-container": "#8b0000",
        "on-secondary-fixed-variant": "#8b0000",
        "on-error-container": "#ffdad6",
        "error-container": "#4a0000",
        "outline-variant": "#3c443c",
      },
      gridTemplateColumns: {
        "16": "repeat(16, minmax(0, 1fr))",
      },
      fontFamily: {
        headline: ["Space Grotesk"],
        body: ["Space Grotesk"],
        label: ["Space Grotesk"],
      },
    },
  },
  plugins: [
    require("@tailwindcss/forms"),
  ],
};
