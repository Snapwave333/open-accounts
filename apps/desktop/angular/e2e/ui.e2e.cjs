const { chromium } = require('playwright');

const base = process.argv[2];
const llmUrl = process.argv[3];
const shotsDir = process.env.OA_SHOTS;
const shot = async (page, name) => {
  if (shotsDir) await page.screenshot({ path: `${shotsDir}/${name}.png`, fullPage: true });
};
if (!base) {
  console.error('usage: node ui.e2e.cjs <bridge-url>');
  process.exit(2);
}

const failures = [];
const check = (cond, message) => {
  if (!cond) failures.push(message);
};

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
  const consoleErrors = [];
  page.on('pageerror', (e) => consoleErrors.push(e.message));
  page.on('console', (m) => {
    if (m.type() === 'error' && !m.text().includes('fonts.g')) consoleErrors.push(m.text());
  });

  await page.addInitScript(() => {
    window.__TAURI_INTERNALS__ = {
      invoke: async (cmd, args) => {
        const res = await fetch('/__invoke', {
          method: 'POST',
          body: JSON.stringify({ cmd, args: args ?? {} }),
        });
        const body = await res.json();
        if (!body.ok) throw body.error;
        return body.value;
      },
      transformCallback: () => 0,
    };
  });

  const tab = (name) => page.locator('.tab-pill', { hasText: name }).first().click();
  const banner = () =>
    page
      .locator('.backend-banner')
      .textContent({ timeout: 500 })
      .catch(() => null);

  try {
    await page.goto(base, { waitUntil: 'networkidle' });
    await page.waitForTimeout(800);
    check((await banner()) === null, `unexpected error banner on load: ${await banner()}`);
    check(
      (await page.locator('.pulse-value').first().textContent())?.trim() !== '0',
      'seeded accounts not shown',
    );

    await tab('Accounts');
    await page.waitForSelector('.data-table tbody tr');
    const before = await page.locator('.data-table tbody tr').count();
    check(before >= 7, `expected seeded accounts, saw ${before}`);
    await page.getByPlaceholder('Handle (e.g. @myaccount)').fill('@e2e_browser');
    await page
      .locator('select')
      .filter({ has: page.locator('option[value="paypal"]') })
      .selectOption('x');
    await page.getByRole('button', { name: '+ Add' }).click();
    await page.waitForTimeout(600);
    check(
      (await page.locator('.data-table tbody tr').count()) === before + 1,
      'new account did not appear',
    );
    check(
      await page.locator('td', { hasText: '@e2e_browser' }).isVisible(),
      'new account row missing',
    );

    const chips = page.locator('.connection-chip');
    await chips.first().waitFor();
    await shot(page, 'accounts');
    check((await chips.count()) >= 5, 'connection statuses missing');
    await page.locator('.connection-chip.ok', { hasText: 'YouTube' }).waitFor();
    check(
      (await page.locator('.connection-chip.ok').count()) === 1,
      'only the connected YouTube account should show as connected',
    );

    const ytRow = page.locator('tr', { hasText: /@ClearlyDrawnLessons/ });
    await ytRow.getByRole('button', { name: 'Sync inbox' }).click();
    await page.waitForTimeout(800);
    check((await banner()) === null, `inbox sync raised: ${await banner()}`);
    await ytRow.getByRole('button', { name: 'Metrics' }).click();
    await page.waitForTimeout(500);
    check((await banner()) === null, `metrics refresh raised: ${await banner()}`);
    await ytRow.locator('.audience-cell', { hasText: '1.2K followers' }).waitFor();
    check(
      (await ytRow.locator('.audience-cell').textContent()).includes('98.8K views'),
      'refreshed metrics not shown',
    );

    await tab('Needs You');
    const fanCard = page.locator('.task-card', { hasText: '@viewer' });
    await fanCard.waitFor();
    const legalCard = page.locator('.task-card', { hasText: '@lawfirm' });
    await legalCard.waitFor();
    await shot(page, 'needs-you');
    check(
      (await legalCard.textContent()).includes('SECURITY OR LEGAL ESCALATION'),
      'legal comment was not escalated',
    );
    check(
      (await legalCard.locator('textarea').inputValue()) === '',
      'escalations must not get an auto-drafted reply',
    );
    check(
      (await fanCard.locator('textarea').inputValue()).length > 0,
      'compliment should arrive with a drafted reply',
    );
    await fanCard.locator('textarea').fill('Thanks so much from the whole studio!');
    await fanCard.getByRole('button', { name: 'Send Reply' }).click();
    await fanCard.waitFor({ state: 'detached' });
    check(await legalCard.isVisible(), 'escalated message should still wait for review');
    await legalCard.getByRole('button', { name: 'Archive' }).click();
    await legalCard.waitFor({ state: 'detached' });
    await tab('Accounts');

    await page
      .locator('td', { hasText: '@e2e_browser' })
      .locator('xpath=..')
      .getByRole('button', { name: 'Remove' })
      .click();
    await page
      .locator('td', { hasText: '@e2e_browser' })
      .locator('xpath=..')
      .getByRole('button', { name: 'Confirm' })
      .click();
    await page.waitForTimeout(600);
    check(
      (await page.locator('.data-table tbody tr').count()) === before,
      'account removal did not persist',
    );

    await tab('Vault');
    await page.getByPlaceholder('Create master password…').fill('browser-e2e-master');
    await page.getByRole('button', { name: 'Create Vault' }).click();
    await page.getByRole('button', { name: 'Lock Vault' }).waitFor();
    await page.getByPlaceholder('Label (e.g. x:access_token)').fill('e2e:note');
    await page.getByPlaceholder('Secret value').fill('super-secret-value');
    await page.getByRole('button', { name: 'Store' }).click();
    await page.locator('td', { hasText: 'e2e:note' }).waitFor();
    await shot(page, 'vault');
    await page.getByRole('button', { name: 'Lock Vault' }).click();
    await page.getByPlaceholder('Master password…').fill('not-the-password');
    await page.getByRole('button', { name: 'Unlock Vault' }).click();
    await page.locator('.vault-error').waitFor();
    check(
      (await page.locator('.vault-error').textContent()).includes('Incorrect'),
      'wrong vault password was not rejected',
    );
    await page.getByPlaceholder('Master password…').fill('browser-e2e-master');
    await page.getByRole('button', { name: 'Unlock Vault' }).click();
    await page.locator('td', { hasText: 'e2e:note' }).waitFor();
    await page.getByRole('button', { name: 'Change Password' }).click();
    await page.getByPlaceholder('Current master password').fill('wrong-current-pass');
    await page.getByPlaceholder('New master password (8+ characters)').fill('rotated-e2e-master');
    await page.getByRole('button', { name: 'Re-encrypt Vault' }).click();
    await page.locator('.vault-error', { hasText: 'Incorrect' }).waitFor();
    await page.getByPlaceholder('Current master password').fill('browser-e2e-master');
    await page.getByPlaceholder('New master password (8+ characters)').fill('rotated-e2e-master');
    await page.getByRole('button', { name: 'Re-encrypt Vault' }).click();
    await page.locator('.vault-error.vault-ok').waitFor();
    await page.getByRole('button', { name: 'Lock Vault' }).click();
    await page.getByPlaceholder('Master password…').fill('browser-e2e-master');
    await page.getByRole('button', { name: 'Unlock Vault' }).click();
    await page.locator('.vault-error', { hasText: 'Incorrect' }).waitFor();
    await page.getByPlaceholder('Master password…').fill('rotated-e2e-master');
    await page.getByRole('button', { name: 'Unlock Vault' }).click();
    await page.locator('td', { hasText: 'e2e:note' }).waitFor();

    await tab('Activity Flow');
    const jobCard = page.locator('.flow-card', { hasText: 'Autopilot marketing cycle' });
    await jobCard.waitFor();
    await jobCard.getByRole('button', { name: 'Run' }).click();
    await page.waitForTimeout(600);
    check((await banner()) === null, `running a job raised: ${await banner()}`);

    const accountSelect = page.locator('.compose-panel select');
    const firstAccount = await accountSelect.locator('option').nth(1).getAttribute('value');
    await accountSelect.selectOption(firstAccount);
    await page.getByPlaceholder('Caption').fill("Behind the scenes of today's episode #animation");
    await page.getByRole('button', { name: 'Schedule post' }).click();
    const slotCard = page.locator('.flow-card', { hasText: 'Behind the scenes' });
    await slotCard.waitFor();
    await shot(page, 'flow');
    check(
      (await slotCard.textContent()).includes('NEEDS APPROVAL'),
      'new post should await approval',
    );
    await tab('Needs You');
    const approval = page.locator('.approval-card', { hasText: 'Behind the scenes' });
    await approval.waitFor();
    check(
      (await page.title()).startsWith('(1)'),
      `window title should count waiting items, got "${await page.title()}"`,
    );
    await approval.getByRole('button', { name: 'Approve' }).click();
    await approval.waitFor({ state: 'detached' });
    check(
      (await page.title()) === 'OpenAccounts',
      'title badge should clear once nothing is waiting',
    );
    await tab('Activity Flow');
    await page
      .locator('.flow-card', { hasText: 'Behind the scenes' })
      .filter({ hasText: 'APPROVED' })
      .waitFor();
    await page
      .locator('.flow-card', { hasText: 'Behind the scenes' })
      .getByRole('button', { name: 'Delete' })
      .click();
    await slotCard.waitFor({ state: 'detached' });

    await page.getByRole('button', { name: 'Resume' }).click();
    await page.locator('.status-label', { hasText: 'AUTOPILOT' }).waitFor();
    await page.getByRole('button', { name: 'Kill Switch' }).click();
    await page.locator('.status-label', { hasText: 'HALTED' }).waitFor();
    await page.getByRole('button', { name: 'Reset Stop' }).click();
    await page.locator('.status-label', { hasText: 'PAUSED' }).waitFor();

    await tab('Agent Chat');
    await page.getByRole('button', { name: '+ Model' }).click();
    await page.locator('.model-form select').selectOption('ollama');
    await page.getByPlaceholder('Model id (e.g. claude-sonnet-5-5)').fill('llama3.2');
    await page.getByPlaceholder('Endpoint (optional)').fill('http://127.0.0.1:9');
    await page.getByRole('button', { name: 'Save model' }).click();
    await page
      .locator('.model-bar select option[value="ollama-llama3.2"]')
      .waitFor({ state: 'attached' });
    await page.locator('.model-bar select').selectOption('ollama-llama3.2');
    await page.waitForTimeout(400);
    check(
      (await page.locator('.model-bar select').inputValue()) === 'ollama-llama3.2',
      'chat model selection did not stick',
    );
    await page.getByPlaceholder(/Message the in-app LM agent/).fill('What needs my attention?');
    await page.getByRole('button', { name: 'Send' }).click();
    await page.locator('.chat-bubble-row.error').waitFor({ timeout: 15000 });
    check(
      (await page.locator('.chat-bubble-row.error').textContent()).includes(
        'No language model answered',
      ),
      'chat did not surface the missing-model error',
    );
    check(
      (await page.locator('.chat-bubble-row.error').textContent()).includes('ollama-llama3.2'),
      'chat did not try the selected model first',
    );
    await page.getByRole('button', { name: 'Remove' }).click();
    await page.waitForTimeout(400);
    check(
      (await page.locator('.model-bar select').inputValue()) === 'nim-cascade',
      'removing the model did not fall back to the default',
    );

    if (llmUrl) {
      await page.getByRole('button', { name: '+ Model' }).click();
      await page.locator('.model-form select').selectOption('openaicompatible');
      await page.getByPlaceholder('Model id (e.g. claude-sonnet-5-5)').fill('e2e-llm');
      await page.getByPlaceholder('Endpoint (optional)').fill(llmUrl);
      await page.getByRole('button', { name: 'Save model' }).click();
      await page
        .locator('.model-bar select option[value="openaicompatible-e2e-llm"]')
        .waitFor({ state: 'attached' });
      await page.locator('.model-bar select').selectOption('openaicompatible-e2e-llm');
      await page.waitForTimeout(300);
      await page.getByRole('button', { name: 'Resume' }).click();
      await page.locator('.status-label', { hasText: 'AUTOPILOT' }).waitFor();
      await page.getByPlaceholder(/Message the in-app LM agent/).fill('Give me a status update');
      await page.getByRole('button', { name: 'Send' }).click();
      const reply = page.locator('.chat-bubble-row', { hasText: 'You have 7 accounts' });
      await reply.waitFor({ timeout: 15000 });
      check(
        !(await reply.textContent()).includes('```action'),
        'action block leaked into the chat bubble',
      );
      check(
        (await reply.textContent()).includes('e2e-scripted-llm'),
        'reply is not attributed to the model that answered',
      );
      await shot(page, 'chat');
      await reply.getByRole('button', { name: 'Pause autopilot' }).click();
      await page.locator('.chat-bubble-row', { hasText: 'Autopilot paused.' }).waitFor();
      await page.locator('.status-label', { hasText: 'PAUSED' }).waitFor();
      check(
        await reply.getByRole('button', { name: '✓ Pause autopilot' }).isDisabled(),
        'executed action can be clicked again',
      );

      await tab('Activity Flow');
      await page.locator('.compose-panel select').selectOption(firstAccount);
      await page
        .getByPlaceholder('What should the post be about?')
        .fill("Tease tomorrow's episode");
      await page.getByRole('button', { name: 'Draft with AI' }).click();
      await page.waitForFunction(
        () => document.querySelector('.compose-panel textarea')?.value.length > 0,
        null,
        { timeout: 15000 },
      );
      check(
        (await page.locator('.compose-panel textarea').inputValue()).includes(
          'You have 7 accounts',
        ),
        'AI caption not inserted',
      );
    } else {
      check(false, 'no scripted LLM url supplied');
    }
  } catch (e) {
    failures.push(`step failed: ${e.message}`);
    await page.screenshot({ path: 'e2e-failure.png' }).catch(() => {});
  } finally {
    await browser.close();
  }

  if (consoleErrors.length) failures.push(`console errors: ${consoleErrors.join(' | ')}`);
  if (failures.length) {
    console.error(failures.join('\n'));
    process.exit(1);
  }
  console.log('ui e2e passed');
})();
