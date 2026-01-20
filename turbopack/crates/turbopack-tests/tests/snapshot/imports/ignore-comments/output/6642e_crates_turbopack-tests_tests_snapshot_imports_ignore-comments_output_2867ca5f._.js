(function() {
  function abort(message) {
    console.error(message);
    throw new Error(message);
  }

  // Security: Ensure this code is running in a worker environment to prevent
  // the worker entrypoint being used as an XSS gadget. If this is a worker, we
  // know that the origin of the caller is the same as our origin.
  if (
    typeof self["WorkerGlobalScope"] === "undefined" ||
    !(self instanceof self["WorkerGlobalScope"])
  ) {
    abort("Worker entrypoint must be loaded in a worker context");
  }

  var url = new URL(location.href);

  // Try querystring first (SharedWorker), then hash (regular Worker)
  var paramsString = url.searchParams.get("params");
  if (!paramsString && url.hash.startsWith("#params=")) {
    paramsString = decodeURIComponent(url.hash.slice("#params=".length));
  }

  if (!paramsString) abort("Missing worker bootstrap config");

  var params = JSON.parse(paramsString);

  // Validate and extract chunk URLs (params[0] must be an array)
  var chunkUrls = Array.isArray(params[0]) ? params[0] : [];

  // Set worker globals before loading chunks
  Object.assign(self, {
    TURBOPACK_NEXT_CHUNK_URLS: chunkUrls,
    TURBOPACK_CHUNK_SUFFIX: typeof params[1] === 'string' ? params[1] : ''
  });

  // Load chunks via importScripts (in reverse order for correct execution)
  if (chunkUrls.length > 0) {
    var scriptsToLoad = [];
    for (var i = 0; i < chunkUrls.length; i++) {
      var chunk = chunkUrls[i];
      // Chunks are relative to the origin.
      var chunkUrl = new URL(chunk, location.origin);
      // Security: Only load scripts from the same origin. This prevents this
      // worker entrypoint from being used as a gadget to load scripts from
      // foreign origins if someone happens to find a separate XSS vector
      // elsewhere on this origin.
      if (chunkUrl.origin !== location.origin) {
        abort("Refusing to load script from foreign origin: " + chunkUrl.origin);
      }
      scriptsToLoad.push(chunkUrl.toString());
    }

    chunkUrls.reverse();
    importScripts.apply(self, scriptsToLoad);
  }
})();