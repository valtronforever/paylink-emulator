# Встановлення й використання емулятора з Inerix

Основне середовище розробки — **macOS**, автоматизовані перевірки — **Linux CI**.
Windows потрібен лише для окремого дослідження справжнього Desktop PayLink.

Емулятор запускається на тому самому комп'ютері, де відкритий браузер з Inerix.
JavaScript сторінки надсилає запити на `http://localhost:13010`. Окремий API
`http://127.0.0.1:13011` призначений для CLI, GUI та тестового runner.
Бекенд Inerix не викликає цей localhost: для бекенда це була б інша машина.

Емулятор готовий для тестування підтримуваних discovery, ping і purchase.
27 записаних HTTP-випадків і 5 перевірок конкурентності/фрагментації зіставлено
зі справжнім PayLink та програмним SSI-терміналом. Підключення саме продуктового
Inerix ще потребує перевірки в його інтеграції [#476](https://github.com/valtronforever/inerix/issues/476).
Реальні платежі не виконуються. Refund/void/reports та платіжна ідемпотентність
не реалізовані. Детальні [межі перевірки](SSI_FINDINGS.md) залишаються чинними.

## 1. Встановлення

### macOS: локальна збірка для розробки

Встановіть Xcode Command Line Tools, якщо вони ще відсутні, та Rust через
[офіційний rustup](https://rust-lang.org/tools/install/):

```sh
xcode-select --install
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
. "$HOME/.cargo/env"

git clone https://github.com/valtronforever/paylink-emulator.git
cd paylink-emulator
git switch main
git pull --ff-only
cargo build --locked -p paylink-emulator -p paylink-gui
```

Якщо Command Line Tools уже встановлені, пропустіть `xcode-select --install`.
Збірка використовує компілятор із `rust-toolchain.toml` і залежності з `Cargo.lock`.
Отримаєте `target/debug/paylink-emulator` та `target/debug/paylink-gui`, зібрані
для архітектури вашого Mac. Для оптимізованої збірки додайте `--release` й замініть
`target/debug` на `target/release` в командах нижче.

Для запуску без компіляції доступні `cli-macos-latest` і `gui-macos-latest` у
[успішному CI гілки main](https://github.com/valtronforever/paylink-emulator/actions/workflows/ci.yml).
Беріть обидва артефакти з одного commit; перевіряйте відповідність архітектури
бінарника вашому Mac (`file ./paylink-emulator`, `uname -m`). Це CI-збірки без
нотаризованого інсталятора. Після розпакування за потреби відновіть executable bit:
`chmod +x ./paylink-emulator ./paylink-gui`.

Локальну `.app` можна зібрати командою `./scripts/package-macos.sh` після debug-збірки
GUI, або `./scripts/package-macos.sh release` після release-збірки. Для приєднання
до CLI-сервера найпростіше запускати GUI-бінарник із Terminal, як описано нижче.

### Windows: готові файли без Rust

1. Відкрийте [GitHub Actions](https://github.com/valtronforever/paylink-emulator/actions/workflows/ci.yml),
   виберіть успішний запуск **CI для гілки main** і запишіть його commit SHA.
2. У секції **Artifacts** завантажте `cli-windows-latest`. Для завантаження
   артефактів через сайт може знадобитися вхід у GitHub.
3. Розпакуйте `paylink-emulator.exe` у `C:\Tools\PayLinkEmulator`.
4. Для віконного керування також завантажте `gui-windows-latest` **із того самого
   запуску** й розпакуйте `paylink-gui.exe` поруч.

Це тестові CI-збірки, не інсталятор Windows і не підписаний реліз. Не потрібно
встановлювати справжній Desktop PayLink, Node.js або SSI-симулятор для звичайного
використання HTTP-емулятора. CI-артефакти мають обмежений строк зберігання;
якщо потрібної збірки вже немає, зберіть відповідний commit із джерел.

Перевірте файли в PowerShell:

```powershell
Set-Location C:\Tools\PayLinkEmulator
.\paylink-emulator.exe --help
Get-FileHash .\paylink-emulator.exe -Algorithm SHA256
```

### Збірка із джерел

Потрібні Git і Rust через [rustup](https://rustup.rs/); `rust-toolchain.toml`
задає версію компілятора. На Windows додатково потрібні Visual Studio Build Tools
із C++ build tools та Windows SDK. На macOS — Xcode Command Line Tools; на Linux
— компілятор/linker C/C++. CLI не потребує графічних бібліотек.

```sh
git clone https://github.com/valtronforever/paylink-emulator.git
cd paylink-emulator
git switch main
git pull --ff-only
cargo build --locked --release -p paylink-emulator
```

Windows-бінарник: `target\release\paylink-emulator.exe`; Linux/macOS:
`target/release/paylink-emulator`. Для GUI додайте
`cargo build --locked --release -p paylink-gui`; платформні графічні залежності
перелічено в [CI workflow](../.github/workflows/ci.yml). Або використайте
готовий GUI-артефакт для своєї ОС.

## 2. Запуск сервера

Приклад використовує платіжний порт **13010**, керівний **13011**. Можна вибрати
інші вільні порти, однаково змінивши команду запуску й URL у Inerix.
Якщо справжній PayLink уже працює, призначте емулятору інший порт.

### macOS

З кореня репозиторію в першому вікні Terminal:

```sh
# Замініть шаблон значенням location.origin відкритої сторінки Inerix.
INERIX_ORIGIN='https://YOUR-INERIX-HOST'
PAYLINK_STATE_DIR="$HOME/Library/Application Support/PayLinkEmulator"
mkdir -p "$PAYLINK_STATE_DIR"
umask 077
export PAYLINK_CONTROL_TOKEN="$(openssl rand -hex 24)"
printf '%s' "$PAYLINK_CONTROL_TOKEN" > "$PAYLINK_STATE_DIR/control-token.txt"

./target/debug/paylink-emulator serve \
  --payment-addr 127.0.0.1:13010 \
  --control-addr 127.0.0.1:13011 \
  --allow-origin "$INERIX_ORIGIN" \
  --journal "$PAYLINK_STATE_DIR/journal.json"
```

Для локального frontend, наприклад на Vite, вкажіть його фактичний origin:
`INERIX_ORIGIN='http://localhost:5173'`. Якщо використовуєте CI-бінарник,
замініть `./target/debug/paylink-emulator` його локальним шляхом.

### Windows

У першому вікні PowerShell:

```powershell
Set-Location C:\Tools\PayLinkEmulator

# Вставте origin відкритого Inerix: значення location.origin у браузері.
# Приклад нижче — шаблон, замініть його фактичним доменом.
$inerixOrigin = 'https://YOUR-INERIX-HOST'

# Локальний токен для CLI/GUI. Збережіть його лише на своєму комп'ютері.
$stateDir = Join-Path $env:LOCALAPPDATA 'PayLinkEmulator'
New-Item -ItemType Directory -Force -Path $stateDir | Out-Null
$env:PAYLINK_CONTROL_TOKEN = [guid]::NewGuid().ToString('N')
Set-Content -LiteralPath (Join-Path $stateDir 'control-token.txt') `
  -Value $env:PAYLINK_CONTROL_TOKEN -NoNewline

.\paylink-emulator.exe serve `
  --payment-addr 127.0.0.1:13010 `
  --control-addr 127.0.0.1:13011 `
  --allow-origin $inerixOrigin `
  --journal (Join-Path $stateDir 'journal.json')
```

Залиште процес запущеним. Перший рядок містить JSON готовності з фактичними URL.
`--allow-origin` — origin **сайту**, а не адреса емулятора: схема, домен і порт
без шляху та завершального `/`. Для кількох сайтів повторіть параметр, наприклад
`--allow-origin https://YOUR-INERIX-HOST --allow-origin http://localhost:5173`.
Після зміни allowlist перезапустіть сервер.

Передавайте той самий токен CLI/GUI. У браузерному payment API він не потрібен.

## 3. Підготовка оплати та керування

### macOS

У другому Terminal перейдіть у той самий репозиторій і підключіться до сервера:

```sh
export PAYLINK_CONTROL_TOKEN="$(cat "$HOME/Library/Application Support/PayLinkEmulator/control-token.txt")"
export PAYLINK_CONTROL_URL='http://127.0.0.1:13011'
./target/debug/paylink-emulator get health
./target/debug/paylink-emulator arm --state approved
```

Тепер виконайте одну оплату з Inerix. Для наступного тесту оберіть один сценарій:

```sh
./target/debug/paylink-emulator arm --state approved --uses 10
./target/debug/paylink-emulator arm --state declined
./target/debug/paylink-emulator arm --state error --error-id terminal_busy
./target/debug/paylink-emulator arm --state approved --authorize-ms 15000
./target/debug/paylink-emulator arm --state approved --delivery disconnect_after_commit
./target/debug/paylink-emulator arm --state manual
```

Не виконуйте всі рядки одразу, якщо не хочете поставити всі сценарії в чергу.
GUI приєднується до вже запущеного сервера з тим самим токеном:

```sh
./target/debug/paylink-gui --connect http://127.0.0.1:13011
```

Після завершення тесту (в окремому Terminal з тим самим токеном або після закриття GUI):

```sh
./target/debug/paylink-emulator get operations
./target/debug/paylink-emulator get journal
./target/debug/paylink-emulator export ./test-results/payment-result.json
./target/debug/paylink-emulator reset
```

Перед export створіть директорію `mkdir -p test-results`. Сервер залишається у
першому Terminal; `Ctrl+C` зберігає журнал і завершує його. Для GUI-керування
використовуйте **Arm next request**, тестову картку й підтвердження покупця.

### Windows

У другому вікні PowerShell:

```powershell
Set-Location C:\Tools\PayLinkEmulator
$stateDir = Join-Path $env:LOCALAPPDATA 'PayLinkEmulator'
$env:PAYLINK_CONTROL_TOKEN = Get-Content `
  -LiteralPath (Join-Path $stateDir 'control-token.txt') -Raw
$env:PAYLINK_CONTROL_URL = 'http://127.0.0.1:13011'

.\paylink-emulator.exe get health
.\paylink-emulator.exe arm --state approved
```

Тепер виконайте **одну оплату з Inerix**. Кожен `arm` за замовчуванням готує одну
операцію. Без сценарію оплата завершується помилкою, автоматичного успіху немає.
Ping і перегляд пристроїв не споживають сценарій оплати.

Виберіть потрібну команду перед наступною оплатою; не виконуйте всі рядки одразу,
якщо не хочете сформувати чергу:

```powershell
.\paylink-emulator.exe arm --state approved --uses 10
.\paylink-emulator.exe arm --state declined
.\paylink-emulator.exe arm --state error --error-id terminal_busy
.\paylink-emulator.exe arm --state approved --authorize-ms 15000
.\paylink-emulator.exe arm --state approved --delivery disconnect_after_commit
.\paylink-emulator.exe arm --state manual
```

`declined` — загальний сценарій моделі. Для точної записаної SSI-відповіді
використовуйте JSON-сценарій з `reference_error`, описаний у [Control API](CONTROL_API.md).
`disconnect_after_commit` моделює втрату відповіді після схвалення: помилка мережі
не означає відсутність змодельованого списання.

Для ручної оплати запустіть GUI з другого вікна, де вже встановлено токен:

```powershell
.\paylink-gui.exe --connect http://127.0.0.1:13011
```

У GUI **Arm next request** готує наступний платіж із сайту. Подайте тестову картку,
підтвердьте дію покупця, за потреби оберіть рішення банку. **Start standalone**
запускає окрему перевірку без Inerix. При підключенні через `--connect` порти й
дозволені origins задаються сервером CLI; другий сервер запускати не потрібно.

Журнал, лічильники та скидання:

```powershell
.\paylink-emulator.exe get state
.\paylink-emulator.exe get operations
.\paylink-emulator.exe get journal
.\paylink-emulator.exe export .\payment-result.json
.\paylink-emulator.exe reset
```

`reset` скасовує поточний тест і очищає стан/чергу; це не банківське повернення.
Оновлення сторінки не скидає емулятор. Для завершення сервера натисніть `Ctrl+C`
у першому вікні: процес збереже журнал. Новий запуск починає новий стан;
експортований журнал автоматично не відновлює операції.

## 4. Підключення JavaScript у Inerix

Налаштуйте базовий URL платіжного провайдера на `http://localhost:13010`.
У стандартній конфігурації є пристрій
`00000000-0000-4000-8000-000000000001`. Суми — додатні цілі числа в копійках:
`100` означає 1,00 грн. Підтримуються `amount` і необов'язковий `merchant_id`.
Інші поля, зокрема `id` чи підтвердження підпису, повертають HTTP 501.

Приклад для клієнтського коду сайту:

```javascript
const paylinkUrl = 'http://localhost:13010';
const terminalId = '00000000-0000-4000-8000-000000000001';

async function paylink(path, {method = 'GET', body, timeoutMs = 180000} = {}) {
  const response = await fetch(`${paylinkUrl}${path}`, {
    method,
    mode: 'cors',
    credentials: 'omit',
    ...(body === undefined ? {} : {
      headers: {'Content-Type': 'application/json'},
      body: JSON.stringify(body),
    }),
    signal: AbortSignal.timeout(timeoutMs),
  });
  const data = await response.json();
  // Збережіть status і тіло: HTTP 503 може містити діагностику термінала.
  return {httpStatus: response.status, ok: response.ok, data};
}

const devices = await paylink('/api/pos/devices');
const ping = await paylink(`/api/pos/${terminalId}/ping`, {timeoutMs: 10000});

// Викликайте з дії користувача, після arm у CLI/GUI.
const payment = await paylink(`/api/pos/${terminalId}/purchase`, {
  method: 'POST',
  body: {amount: 100},
});
if (payment.ok && payment.data.success === true) {
  console.log('Схвалено', payment.data.id, payment.data.result);
} else {
  console.error('Відповідь PayLink', payment.httpStatus, payment.data);
}
```

Мережеву помилку/AbortError обробляйте окремо від JSON-відмови. Клієнтський timeout
не скасовує операцію. Не повторюйте purchase автоматично після невідомого результату:
повтор може створити ще одну змодельовану оплату. Перевірте журнал через CLI/GUI.
180 секунд у прикладі — клієнтський бюджет тесту; змінюйте його відповідно до сценарію.

Токен керівного API не додається до JS, збірки сайту або запитів payment API.
Сайт звертається лише до платіжного порту 13010, а CLI/GUI — до 13011.

## 5. HTTPS-сайт, CORS і дозволи браузера

HTTPS у Inerix та HTTP на loopback — очікувана схема цього емулятора. Локальні
`localhost`/`127.0.0.1` мають спеціальний статус довіри, але це не скасовує CORS
і перевірки доступу до локальної мережі. Якщо браузер запитує дозвіл для Inerix
на доступ до localhost/локальної мережі, надайте його для цього сайту.
При відмові перевірте дозволи сайту й корпоративну політику браузера.
Джерела: [Chrome Local Network Access](https://developer.chrome.com/blog/local-network-access),
[MDN: локальний мережевий доступ](https://developer.mozilla.org/en-US/docs/Web/Security/Defenses/Local_network_access),
[MDN: mixed content і loopback](https://developer.mozilla.org/en-US/docs/Web/Security/Defenses/Mixed_content).

Якщо Inerix використовує CSP, його `connect-src` має дозволяти точний локальний URL,
наприклад `http://localhost:13010`; додайте його до наявної політики, зберігаючи інші
потрібні джерела. `http://localhost:13010` та `http://127.0.0.1:13010` — різні URL
для браузерних політик. Не використовуйте `mode: 'no-cors'`: тіло такої відповіді
недоступне JavaScript. Вимикати захист браузера для цього підключення не потрібно.

Перевірено звичайні браузерні HTTP-запити та відмову HTTPS→loopback без дозволу.
Успішний доступ із фактичного HTTPS-origin Inerix з дозволом LNA ще треба перевірити
в його середовищі; merge емулятора не означає завершення цієї інтеграційної перевірки.

## 6. Linux CI

Основні автоматизовані перевірки вже налаштовані в
[GitHub Actions](../.github/workflows/ci.yml): Linux headless/API, Node SSI/recorder,
Chromium і Docker. Додаткові macOS/Windows jobs перевіряють переносимість CLI/GUI.
Звичайний Linux CI не запускає справжній Windows PayLink; він використовує
версійні fixtures й незалежні тести моделі.

На Ubuntu runner потрібні Rust із зафіксованою версією, Node **24**, OpenSSL та
залежності Chromium. Приклад кроків із кореня репозиторію:

```sh
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked -p paylink-emulator
npm ci
npm run test:lab
npx playwright install --with-deps chromium
CI=true npm run test:browser
```

Browser fixtures самі запускають окремий емулятор на вільних портах для кожного
тесту, керують сценаріями й завершують процес. Результати — у `test-results/`.
Тестовий HTTPS harness використовує тимчасовий self-signed сертифікат і
`ignoreHTTPSErrors` лише для нього; це не перевірка сертифіката чи LNA-дозволу
продуктового Inerix. Окремий SSI browser recorder не вимикає ці перевірки.

Для strict-порівняння з усіма записаними reference fixtures після збірки CLI:

```bash
set -euo pipefail
mkdir -p test-results
export PAYLINK_CONTROL_TOKEN="$(openssl rand -hex 24)"
./target/debug/paylink-emulator serve \
  --payment-addr 127.0.0.1:0 --control-addr 127.0.0.1:0 \
  --journal test-results/reference-journal.json \
  > test-results/reference-ready.jsonl 2> test-results/reference-stderr.log &
PAYLINK_TEST_PID=$!
trap 'kill -INT "$PAYLINK_TEST_PID" 2>/dev/null || true; wait "$PAYLINK_TEST_PID" || true' EXIT
for attempt in $(seq 1 100); do
  if test -s test-results/reference-ready.jsonl; then break; fi
  if ! kill -0 "$PAYLINK_TEST_PID" 2>/dev/null; then cat test-results/reference-stderr.log; exit 1; fi
  sleep 0.1
done
export PAYLINK_PAYMENT_URL="$(node -p "JSON.parse(require('fs').readFileSync('test-results/reference-ready.jsonl','utf8').trim()).payment_url")"
export PAYLINK_CONTROL_URL="$(node -p "JSON.parse(require('fs').readFileSync('test-results/reference-ready.jsonl','utf8').trim()).control_url")"
node scripts/differential.mjs --require-reference
```

`--require-reference` не дозволяє зарахувати відсутні fixtures як успішний тест.
Збережіть `test-results/differential.json`, журнал і stderr як CI-артефакти.
Для власних браузерних тестів Inerix передайте серверу точний `--allow-origin`
тестового сайту, задайте йому `PAYLINK_PAYMENT_URL` і виконайте `arm` через runner.
Керівний токен лишається в runner, а не у frontend.

Нативний headless-процес найпростіше запускати поряд із browser runner. У Docker
listener слухає loopback **контейнера**; звичайного `-p` недостатньо. Якщо потрібен
контейнер, browser runner має бути в тому самому network namespace — див.
[контейнерну інструкцію](CONTAINERS.md).

## 7. Швидка діагностика

| Симптом | Що перевірити |
|---|---|
| Connection refused | Сервер працює, порт збігається з readiness і URL Inerix. |
| Порт зайнятий | Не запускайте два сервери на одному порту; змініть порт і URL сайту. |
| `localhost` не підключається | Спробуйте `http://127.0.0.1:13010`: приклад сервера слухає IPv4. Якщо середовище використовує лише `::1`, узгодьте IPv6 bind і URL. |
| CLI працює, браузер — ні | CLI не перевіряє браузерні CORS/LNA/CSP; дивіться Console і Network. |
| Origin denied / preflight 403 | `--allow-origin` має точно збігатися з `location.origin` сайту. |
| Permission denied / loopback / local network | Дозвіл сайту на локальний доступ або керована політика браузера. |
| CSP `connect-src` | Додайте локальний платіжний URL до політики самого Inerix. |
| Немає сценарію | Виконайте `arm` перед оплатою; попередній сценарій міг уже використатися. |
| HTTP 503, code 9009 | Пристрій зайнятий; дочекайтесь завершення першої операції. |
| HTTP 501 | Inerix надсилає непідтримуваний маршрут або поля; звірте контракт. |
| Control API 401/403 | Той самий токен, правильний control URL; не викликайте control із браузера. |
| GUI не керує оплатою сайту | GUI має бути підключений через `--connect` саме до control URL цього сервера. |

Перевірка платіжного порту поза браузером:

```sh
curl --fail-with-body http://localhost:13010/api/pos/devices
curl --fail-with-body http://localhost:13010/api/pos/00000000-0000-4000-8000-000000000001/ping
```

Або у PowerShell:

```powershell
Invoke-RestMethod http://localhost:13010/api/pos/devices
Invoke-RestMethod http://localhost:13010/api/pos/00000000-0000-4000-8000-000000000001/ping
```

Для дослідження самого PayLink існує окремий [SSI-стенд](WINDOWS_SSI_LAB.md).
Для щоденного тестування Inerix достатньо одного HTTP-емулятора й CLI/GUI.
