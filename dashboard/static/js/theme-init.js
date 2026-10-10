/*
 * Applies the stored theme before first paint so a returning User never sees
 * the wrong theme while the WebAssembly application loads. The storage key
 * must match `STORAGE_KEY` in `src/ui/theme.rs`.
 */
(function () {
	"use strict";
	try {
		var stored = localStorage.getItem("rc-theme");
		if (stored === "light" || stored === "dark") {
			document.documentElement.setAttribute("data-theme", stored);
		}
	} catch (error) {
		/* Storage can be blocked; the system theme still applies. */
	}
})();
