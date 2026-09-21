# Емулятор PayLink

Кросплатформений емулятор еквайрингового термінала на Rust для ручних перевірок і UI-тестів у браузері. Цільовий профіль — **Checkbox Desktop PayLink 2.1.20 win-x86**. Сам емулятор запускається на Linux, macOS та Windows.

Профіль поки має статус **unverified**: версію та SHA-256 офіційного інсталятора зафіксовано, але API-відповіді й часові правила ще не порівняно з реальним терміналом. Це видно в CLI, API, UI та звіті покриття. Зі збірки 2.1.20.10 також отримано відтворюваний перелік 169 кодів статусів; це включає проміжні стани й не означає покриття 169 помилок. Емулятор не звертається до банку й не проводить справжніх платежів.

## Запуск

Потрібен [Rust](https://rustup.rs/). Версію компілятора зафіксовано у `rust-toolchain.toml`.

```sh
cargo build --locked -p paylink-emulator
export PAYLINK_CONTROL_TOKEN='local-test-token-at-least-16-chars'
cargo run --locked -p paylink-emulator -- serve \
  --allow-origin https://your-test-ui.example \
  --journal ./journal.json
```

У PowerShell замість `export`: `$env:PAYLINK_CONTROL_TOKEN = 'local-test-token-at-least-16-chars'`.

За замовчуванням платіжний API слухає `127.0.0.1:3000`, керівний — `127.0.0.1:3001`. Перший рядок stdout містить JSON готовності з фактичними адресами. Для паралельних тестів використовуйте окремий процес і порти `0`, щоб ОС вибрала вільні.

Наступну оплату можна налаштувати параметрами CLI:

```sh
cargo run -p paylink-emulator -- arm --state approved --authorize-ms 1500
cargo run -p paylink-emulator -- arm --state error --error-id terminal_busy
cargo run -p paylink-emulator -- arm --state manual
cargo run -p paylink-emulator -- arm --file examples/lost-reply.json
```

У strict-режимі відсутність сценарію ніколи не перетворюється на автоматичну успішну оплату. Суми передаються цілим числом у копійках. Для стандартного тестового пристрою клієнт надсилає `POST /api/pos/00000000-0000-4000-8000-000000000001/purchase` із JSON `{"amount":100}`.

Платіжний запит підтримує `amount` і необов’язковий `merchant_id`. Інші параметри, зокрема реальний PayLink `id` та підтвердження підпису, повертають HTTP 501 до початку операції. Їхню поведінку ще потрібно відкалібрувати; деталі — у [контракті сумісності](docs/COMPATIBILITY.md).

## Віконний додаток

```sh
cargo run --locked -p paylink-gui
# Приєднання до вже запущеного сервера з його токеном:
cargo run --locked -p paylink-gui -- --connect http://127.0.0.1:3001
```

Додаток має зображення умовного термінала, дисплей, цифрову клавіатуру, OK/Cancel та подачу тестової картки. Кнопка **Arm next request** готує сценарій для браузерної оплати; **Start standalone** запускає ручну перевірку без іншої програми. Під час активної оплати її суму не можна змінити клавіатурою.

У ручному режимі операція чекає дій покупця. Окремо можна ввімкнути ручне рішення банку, змінити фазові затримки, результат, помилку, фазу її виникнення або спосіб втрати/пошкодження відповіді. Вибрана конфігурація фіксується на початку операції; подальші зміни стосуються наступного сценарію.

Усі 13 поширених випадків із документації доступні для вибору. Вимкнений PayLink відтворюється зупинкою платіжного listener; помилка драйвера 9011 — окремим станом недоступного пристрою, без вигаданого коду платіжної відповіді. Журнал і лічильники дозволяють розрізняти запит, прийняту операцію, списання та доставку відповіді.

GUI потребує графічного середовища. Звичайні `cargo build` і `cargo test` не збирають GPUI, тому headless CI не потребує GPU. macOS-пакет: після збірки виконайте `./scripts/package-macos.sh`.

## Перевірки

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --all --check
npm ci
npx playwright install chromium
cargo build --locked -p paylink-emulator
npm run test:browser
```

Браузерні тести використовують HTTPS-сторінку, реальні локальні з'єднання та окремий процес на тест. Перехоплення `page.route()` не використовується. Для тестового сертифіката потрібен OpenSSL. Артефакти зберігаються в `test-results/`.

Токен керівного API передається лише CLI, native UI або test runner; браузерний код його не отримує. Повтор платежу може спричинити повторне змодельоване списання — платіжна ідемпотентність не вигадується. Reload сторінки не скидає термінал. `reset` закриває старі з'єднання, очищає сценарії та змінює generation.

## Документація

- [Повний README англійською](README.md).
- [Керівний API](docs/CONTROL_API.md) і [OpenAPI 3.1](docs/control-openapi.json).
- [Сценарії](examples/), [контейнери](docs/CONTAINERS.md), [тестування](docs/TESTING.md).
- [Версійний профіль](profiles/desktop-paylink-2.1.20-win-x86/manifest.json), [межі сумісності та калібрування](docs/COMPATIBILITY.md).
- [Задачі репозиторію](https://github.com/valtronforever/paylink-emulator/issues), [інтеграція Inerix](https://github.com/valtronforever/inerix/issues/476).
- Офіційні джерела: [підключення PayLink](https://wiki.checkbox.ua/app/pc/portal_acquiring), [поширені помилки](https://wiki.checkbox.ua/app/pc/desktop_paylink_errors).
