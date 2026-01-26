#!/usr/bin/env node
/**
 * Minimal Puppeteer script to verify a test page.
 * 
 * Usage: node check.mjs <url>
 * 
 * Expects the page to have an element with id="result" containing
 * "WASM_BODGE_TEST_PASSED" when the test succeeds.
 */

import puppeteer from 'puppeteer';

const EXPECTED_RESULT = 'WASM_BODGE_TEST_PASSED';
const TIMEOUT_MS = 30000;

const url = process.argv[2];
if (!url) {
  console.error('Usage: node check.mjs <url>');
  process.exit(1);
}

const browser = await puppeteer.launch({
  args: ['--no-sandbox', '--disable-setuid-sandbox'],
  headless: true,
});

try {
  const page = await browser.newPage();
  page.setDefaultTimeout(TIMEOUT_MS);

  // Log browser console for debugging
  page.on('console', msg => console.log(`[browser] ${msg.text()}`));
  page.on('pageerror', err => console.error(`[browser error] ${err.message}`));

  await page.goto(url, { waitUntil: 'networkidle0', timeout: TIMEOUT_MS });
  await page.waitForSelector('#result', { timeout: TIMEOUT_MS });

  const result = await page.evaluate(() => {
    const el = document.querySelector('#result');
    return el ? el.textContent : null;
  });

  if (result !== EXPECTED_RESULT) {
    console.error(`Test failed: expected "${EXPECTED_RESULT}", got "${result}"`);
    process.exit(1);
  }

  console.log('Browser test passed!');
} finally {
  await browser.close();
}
