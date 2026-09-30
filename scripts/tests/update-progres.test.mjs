/**
 * Le pourcentage de la barre de mise a jour (src/lib/stores/updateProgres.ts).
 * Signale le 2026-09-30 : la barre montait a 300 %.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { pourcentage } from "../../src/lib/stores/updateProgres.ts";

test("la barre ne depasse jamais 100 %", () => {
  assert.equal(pourcentage(300, 100), 100);
  assert.equal(pourcentage(50, 100), 50);
  assert.equal(pourcentage(-5, 100), 0);
});

test("sans taille connue, on n'invente pas de pourcentage", () => {
  assert.equal(pourcentage(10, null), null);
  assert.equal(pourcentage(10, 0), null);
});
