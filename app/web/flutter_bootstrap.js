{{flutter_js}}
{{flutter_build_config}}

_flutter.loader.load({
  config: {
    renderer: "canvaskit",
    canvasKitBaseUrl: new URL("canvaskit/", document.baseURI).href,
    canvasKitVariant: "full",
    fontFallbackBaseUrl: new URL("fonts/", document.baseURI).href,
  },
});
