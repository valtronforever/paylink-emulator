import {defineConfig} from '@playwright/test';
export default defineConfig({
  testDir: './tests/browser', timeout: 30000, fullyParallel: true,
  workers: process.env.CI ? 2 : 3,
  outputDir: 'test-results/browser',
  reporter: [['list'], ['json', {outputFile: 'test-results/browser-report.json'}]],
  use: {browserName:'chromium',headless:true,ignoreHTTPSErrors:true,trace:'retain-on-failure',screenshot:'only-on-failure'},
});
