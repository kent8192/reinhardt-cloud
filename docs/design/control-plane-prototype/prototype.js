/*
 * Prototype behaviour only: theme switching, copy buttons, dialogs, toasts
 * and menu dismissal. The Pages implementation replaces this with component
 * state; none of it is part of the design contract.
 */
(function () {
	"use strict";

	var root = document.documentElement;
	var STORAGE_KEY = "rc-prototype-theme";

	function systemTheme() {
		return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
	}

	function currentTheme() {
		return root.getAttribute("data-theme") || systemTheme();
	}

	// Apply a stored choice before first paint to avoid a flash of the wrong theme.
	try {
		var stored = localStorage.getItem(STORAGE_KEY);
		if (stored === "light" || stored === "dark") {
			root.setAttribute("data-theme", stored);
		}
	} catch (e) {
		/* Storage can be blocked; the system theme still applies. */
	}

	function syncThemeButtons() {
		var next = currentTheme() === "dark" ? "light" : "dark";
		document.querySelectorAll("[data-theme-toggle]").forEach(function (button) {
			button.textContent = next === "dark" ? "Switch to dark theme" : "Switch to light theme";
		});
	}

	document.addEventListener("DOMContentLoaded", function () {
		syncThemeButtons();

		document.querySelectorAll("[data-theme-toggle]").forEach(function (button) {
			button.addEventListener("click", function () {
				var next = currentTheme() === "dark" ? "light" : "dark";
				root.setAttribute("data-theme", next);
				try {
					localStorage.setItem(STORAGE_KEY, next);
				} catch (e) {
					/* Ignore blocked storage. */
				}
				syncThemeButtons();
			});
		});

		document.querySelectorAll("[data-copy-target]").forEach(function (button) {
			var label = button.textContent;
			button.addEventListener("click", function () {
				var source = document.getElementById(button.getAttribute("data-copy-target"));
				if (!source || !navigator.clipboard) {
					return;
				}
				navigator.clipboard.writeText(source.textContent.trim()).then(function () {
					button.textContent = "Copied";
					window.setTimeout(function () {
						button.textContent = label;
					}, 1600);
				});
			});
		});

		document.querySelectorAll("[data-dialog-open]").forEach(function (button) {
			button.addEventListener("click", function () {
				var dialog = document.getElementById(button.getAttribute("data-dialog-open"));
				if (dialog && dialog.showModal) {
					dialog.showModal();
				}
			});
		});

		document.querySelectorAll("[data-dialog-close]").forEach(function (button) {
			button.addEventListener("click", function () {
				var dialog = button.closest("dialog");
				if (dialog) {
					dialog.close();
				}
			});
		});

		document.querySelectorAll("[data-toast-dismiss]").forEach(function (button) {
			button.addEventListener("click", function () {
				var toast = button.closest(".toast");
				if (toast) {
					toast.hidden = true;
				}
			});
		});

		document.querySelectorAll("[data-pressed-toggle]").forEach(function (button) {
			button.addEventListener("click", function () {
				var pressed = button.getAttribute("aria-pressed") === "true";
				button.setAttribute("aria-pressed", pressed ? "false" : "true");
			});
		});

		document.querySelectorAll("[data-enable-with]").forEach(function (button) {
			var box = document.getElementById(button.getAttribute("data-enable-with"));
			if (box) {
				box.addEventListener("change", function () {
					button.disabled = !box.checked;
				});
			}
		});

		document.querySelectorAll("form[data-prototype]").forEach(function (form) {
			form.addEventListener("submit", function (event) {
				event.preventDefault();
			});
		});

		document.querySelectorAll(".log-body").forEach(function (log) {
			log.scrollTop = log.scrollHeight;
		});

		function closeMenus(except) {
			document.querySelectorAll("details.switcher[open], details.user-menu[open]").forEach(function (menu) {
				if (menu !== except) {
					menu.removeAttribute("open");
				}
			});
		}

		document.addEventListener("click", function (event) {
			closeMenus(event.target.closest("details.switcher, details.user-menu"));
		});

		document.addEventListener("keydown", function (event) {
			if (event.key === "Escape") {
				closeMenus(null);
			}
		});
	});
})();
