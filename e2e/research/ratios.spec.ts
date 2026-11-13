import { test, expect } from "@playwright/test";
import { sampleIndiaData } from "../../apps/frontend/src/features/research/india-summary.fixture";

test.use({ trace: "off" });
test("provider ratios load explicitly and isolate failures without changing the financial report", async ({ context, page }, info) => {
  const state=await (await context.request.post('/api/v1/profiles/get_profile_state',{data:{}})).json();
  const grant=await (await context.request.post('/api/v1/profiles/unlock_profile',{data:{profileId:state.profiles[0].id}})).json();
  const headers={"x-wf-profile-scope":grant.scopeId};
  await context.request.put('/api/v1/settings',{headers,data:{onboardingCompleted:true,language:'en',baseCurrency:'INR',timezone:'Asia/Kolkata',syncEnabled:false,autoUpdateCheckEnabled:false}});
  // Real, newly wired handler must reject the test network mode before reading
  // a credential or making a provider request. No owner token is used.
  const blocked=await context.request.post('/api/v1/research/india/ratios',{headers,data:{instrumentKey:'NSE_EQ|INE213A01029'}});
  expect(blocked.status()).toBe(409);expect(await blocked.text()).toContain('INDIA_NETWORK_DISABLED_IN_TEST');
  const source=sampleIndiaData();let calls=0;let fail=false;let chartCalls=0;let hold=false;
  let release: (()=>void)|undefined;
  await page.route('**/api/v1/research/india/connection',r=>r.fulfill({json:{configured:true,expiresOn:'2027-01-01'}}));
  await page.route('**/api/v1/research/india/search',r=>r.fulfill({json:{listings:[source.listing],retrievedAt:source.retrievedAt}}));
  await page.route('**/api/v1/research/india/analyse',r=>r.fulfill({json:source}));
  await page.route('**/api/v1/research/india/chart',r=>{chartCalls++;return r.fulfill({json:{listing:source.listing,range:'month',interval:'1 day',candles:[],quote:{price:123,lastTradeAt:'2026-10-27T07:00:00Z'},retrievedAt:'2026-10-27T07:00:00Z'}});});
  await page.route('**/api/v1/research/india/ratios',async r=>{calls++;if(hold)await new Promise<void>(resolve=>{release=resolve;});return fail?r.fulfill({status:502,json:{error:'SOURCE_UNAVAILABLE'}}):r.fulfill({json:{listing:source.listing,retrievedAt:'2026-10-27T07:00:00Z',ratios:[{name:'P/E',company:20,sector:15},{name:'ROE',company:-5,sector:null}]}});});
  await page.clock.install({time:new Date('2026-10-27T07:00:00Z')});
  for(const width of [390,1280]){
    await page.setViewportSize({width,height:width===390?844:900});await page.goto('/research');
    await page.getByLabel('Market for stock research').selectOption('india');await page.getByLabel('Indian company name, symbol or ISIN').fill('TEST');
    await page.getByRole('button',{name:'Search India',exact:true}).click();await page.getByLabel('Verified company listing').selectOption(source.listing.key);await page.getByRole('button',{name:'Analyse India',exact:true}).click();
    const panel=page.getByRole('region',{name:'Company and sector ratios — Upstox'});
    expect(calls).toBe(width===390?0:2);await expect(panel.getByRole('table')).toHaveCount(0);
    await page.getByText('Price and earnings check',{exact:true}).click();await page.getByLabel('Share price (INR/share)',{exact:true}).fill('123.45');
    await page.getByRole('button',{name:'Open chart',exact:true}).click();await expect(page.getByRole('button',{name:'Refresh chart',exact:true})).toBeEnabled();await page.getByLabel('Update every 15 seconds').check();
    const priorChartCalls=chartCalls;hold=true;release=undefined;
    fail=false;await panel.getByRole('button',{name:'Load provider ratios',exact:true}).click();
    await expect(panel.getByRole('button',{name:'Loading ratios…',exact:true})).toBeVisible();await expect.poll(()=>release!==undefined).toBe(true);
    await page.clock.runFor(16000);expect(chartCalls).toBe(priorChartCalls);hold=false;release!();
    await expect(page.getByLabel('Share price (INR/share)',{exact:true})).toHaveValue('123.45');await expect(panel.getByRole('table')).toContainText('20×');await expect(panel.getByRole('table')).toContainText('-5%');await expect(panel).toContainText('not the valuation date');await expect(panel).toContainText('not established');
    expect(await panel.getByRole('table').evaluate(el=>el.parentElement!.scrollWidth<=el.parentElement!.clientWidth)).toBe(true);await panel.getByRole('table').scrollIntoViewIfNeeded();expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
    await page.screenshot({path:info.outputPath(`ratios-${width}.png`)});
    fail=true;await panel.getByRole('button',{name:'Load provider ratios',exact:true}).click();await expect(panel.getByRole('alert')).toBeVisible();await expect(panel.getByRole('table')).toHaveCount(0);
    await expect(page.getByText('Income summaries · ₹ crore',{exact:true})).toBeVisible();
    await page.getByRole('button',{name:'Clear India result',exact:true}).click();await expect(panel).toHaveCount(0);await page.reload();await expect(panel).toHaveCount(0);
  }
  expect(calls).toBe(4);
});
