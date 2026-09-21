import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
const profile='profiles/desktop-paylink-2.1.20-win-x86';
const catalog=JSON.parse(readFileSync(`${profile}/errors.json`));
const manifest=JSON.parse(readFileSync(`${profile}/manifest.json`));
const rust=readFileSync('test-results/rust-tests.log','utf8');
const browser=JSON.parse(readFileSync('test-results/browser-report.json'));
const specs=[];
function collect(suites){for(const suite of suites){specs.push(...(suite.specs||[]));collect(suite.suites||[])}}
collect(browser.suites||[]);
const rustPassed=name=>new RegExp(`test ${name} \\.\\.\\. ok`).test(rust);
const browserPassed=title=>specs.some(s=>s.title===title && s.ok && s.tests.every(t=>t.status==='expected'));
const entries=catalog.map(error=>{
  const unit=error.category==='terminal'?'every_documented_terminal_error_has_a_scenario':error.category==='setup_only'?'setup_fault_is_unavailability_not_fake_payment_code':null;
  const api=error.category==='terminal'?'every_terminal_error_travels_over_real_http_and_recovers':error.category==='transport'?'controlled_time_manual_actions_busy_reset_and_listener_recovery':'setup_fault_is_visible_without_claiming_a_payment_driver_code';
  const ui=error.category==='terminal'?'all documented terminal errors are visible and recoverable':'not-running and setup-only errors have distinct observable consequences';
  return {...error,model_test:unit,api_test:api,browser_test:ui,
    model_exercise:unit?(rustPassed(unit)?'pass':'failed'):'not_applicable',
    api_exercise:api?(rustPassed(api)?'pass':'failed'):'pending',
    browser_exercise:ui?(browserPassed(ui)?'pass':'failed'):'pending',
    reference_compatibility:'unverified/blocked',inerix_ui:'pending'};
});
const report={profile:manifest.id,installer_hash_verified:manifest.installer.hash_verified,compatibility:manifest.compatibility,catalog_count:entries.length,verified_reference_cases:0,entries};
mkdirSync('test-results',{recursive:true});writeFileSync('test-results/coverage.json',JSON.stringify(report,null,2));
if(entries.length!==13||new Set(entries.map(e=>e.id)).size!==13||entries.some(e=>[e.model_exercise,e.api_exercise,e.browser_exercise].includes('failed')))process.exitCode=1;
console.log(`Catalog ${entries.length}; reference verified 0; report: test-results/coverage.json`);
