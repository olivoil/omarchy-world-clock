#!/usr/bin/env node

import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const OUTPUT = fileURLToPath(new URL("../data/featured-cities.json", import.meta.url));
const CHECK = process.argv.includes("--check");
const SOURCE = {
  name: "Natural Earth 1:10m populated places",
  url: "https://raw.githubusercontent.com/nvkelso/natural-earth-vector/v5.1.2/geojson/ne_10m_populated_places.geojson",
  sha256: "9b8e3de09048ef00dfc70357dbb9fa324493f214b5e0ae4daf1aa79a8d10116b",
};
const TITLE_OVERRIDES = new Map([
  ["Delhi", "New Delhi"],
  ["New York City", "New York"],
  ["Washington", "Washington DC"],
]);

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

async function loadSource() {
  const response = await fetch(SOURCE.url);
  if (!response.ok)
    throw new Error(`Could not download ${SOURCE.name}: HTTP ${response.status}`);
  const text = await response.text();
  const actualHash = sha256(text);
  if (actualHash !== SOURCE.sha256)
    throw new Error(`${SOURCE.name} checksum mismatch: ${actualHash}`);
  return JSON.parse(text);
}

function textValue(...values) {
  for (const value of values) {
    const text = String(value || "").replace(/\s+/g, " ").trim();
    if (text) return text;
  }
  return "";
}

function isFeatured(properties) {
  const sourceZoom = Number(properties.MIN_ZOOM);
  const population = Number(properties.POP_MAX || 0);
  const isCapital = Number(properties.ADM0CAP || 0) === 1;
  const isWorldCity = Number(properties.WORLDCITY || 0) === 1;
  return sourceZoom <= 4
    || sourceZoom <= 5.7 && isCapital && population >= 200000
    || sourceZoom <= 5.1 && isWorldCity;
}

function globeZoom(sourceZoom) {
  return Math.max(0.75, Math.min(4.45,
    0.75 + (Number(sourceZoom) - 1.7) * 0.85));
}

function cityFromFeature(feature) {
  const properties = feature.properties || {};
  const latitude = Number(properties.LATITUDE);
  const longitude = Number(properties.LONGITUDE);
  const sourceZoom = Number(properties.MIN_ZOOM);
  const sourceTitle = textValue(properties.NAME_EN, properties.NAMEASCII, properties.NAME);
  const title = TITLE_OVERRIDES.get(sourceTitle) || sourceTitle;
  if (!title || !Number.isFinite(latitude) || !Number.isFinite(longitude)
      || !Number.isFinite(sourceZoom) || !isFeatured(properties)) return null;
  const sourceTimezone = textValue(properties.TIMEZONE);
  return {
    title,
    latitude,
    longitude,
    minimum_zoom: Number(globeZoom(sourceZoom).toFixed(3)),
    source_timezone: sourceTimezone || null,
    source_label_rank: Number(properties.LABELRANK || 99),
    source_population: Number(properties.POP_MAX || 0),
  };
}

const collection = await loadSource();
const rankedCities = collection.features
  .map(cityFromFeature)
  .filter(Boolean)
  .sort((left, right) => left.minimum_zoom - right.minimum_zoom
    || left.source_label_rank - right.source_label_rank
    || right.source_population - left.source_population
    || left.title.localeCompare(right.title));
const cities = rankedCities.filter((city, index) => !rankedCities
  .slice(0, index)
  .some(previous => previous.title === city.title
    && Math.abs(previous.latitude - city.latitude) < 1
    && Math.abs(previous.longitude - city.longitude) < 1));
const output = `${JSON.stringify(cities, null, 2)}\n`;

if (CHECK) {
  const current = readFileSync(OUTPUT, "utf8");
  if (current !== output) {
    console.error("data/featured-cities.json is out of date; run scripts/build-featured-cities.mjs.");
    process.exit(1);
  }
} else {
  writeFileSync(OUTPUT, output);
  console.log(`Wrote ${cities.length} ranked cities to ${OUTPUT}.`);
}
