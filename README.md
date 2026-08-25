## Check discussions





-------------------------

# 🌟 vulkan-cocoa-core (QMV Core Engine)

## 🗺️ Select Language / Выберите язык
* [English (#-english)](#-english)
* [Русский (#-русский)](#-русский)
* [Українська (#-українська)](#-українська)
* [Беларуская (#-беларуская)](#-беларуская)
* [Italiano (#-italiano)](#-italiano)

---

## 📊 System Architecture / Архитектура системы

<p align="center">
<svg xmlns="http://w3.org" viewBox="0 0 800 360" width="100%" height="100%" style="background:#0d1117; font-family:system-ui,sans-serif;">
  <!-- Headers -->
  <text x="130" y="40" fill="#8b949e" font-size="14" font-weight="bold" text-anchor="middle">APPLE ENVIRONMENT</text>
  <text x="400" y="40" fill="#58a6ff" font-size="14" font-weight="bold" text-anchor="middle">TRANSLATION LAYER (QMV)</text>
  <text x="670" y="40" fill="#ff5555" font-size="14" font-weight="bold" text-anchor="middle">NATIVE HARDWARE</text>
  
  <!-- Apple Layer -->
  <rect x="30" y="70" width="200" height="60" rx="8" fill="#21262d" stroke="#30363d" stroke-width="2"/>
  <text x="130" y="105" fill="#c9d1d9" font-size="14" font-weight="bold" text-anchor="middle">Cocoa / AppKit</text>
  
  <rect x="30" y="150" width="200" height="60" rx="8" fill="#21262d" stroke="#30363d" stroke-width="2"/>
  <text x="130" y="185" fill="#c9d1d9" font-size="14" font-weight="bold" text-anchor="middle">Metal 3 (AIR Bytecode)</text>
  
  <rect x="30" y="230" width="200" height="60" rx="8" fill="#21262d" stroke="#30363d" stroke-width="2"/>
  <text x="130" y="265" fill="#c9d1d9" font-size="14" font-weight="bold" text-anchor="middle">CoreAnimation</text>

  <!-- Connectors Left to Middle -->
  <path d="M 230 100 L 300 110" stroke="#8b949e" stroke-width="2" fill="none" stroke-dasharray="4"/>
  <path d="M 230 180 L 300 180" stroke="#8b949e" stroke-width="2" fill="none"/>
  <path d="M 230 260 L 300 250" stroke="#8b949e" stroke-width="2" fill="none" stroke-dasharray="4"/>

  <!-- QMV Layer -->
  <rect x="300" y="70" width="200" height="220" rx="12" fill="#161b22" stroke="#58a6ff" stroke-width="3"/>
  
  <rect x="315" y="90" width="170" height="40" rx="6" fill="#21262d" stroke="#58a6ff"/>
  <text x="400" y="115" fill="#58a6ff" font-size="12" text-anchor="middle">Kqueue Event Loop</text>
  
  <rect x="315" y="145" width="170" height="40" rx="6" fill="#21262d" stroke="#58a6ff"/>
  <text x="400" y="170" fill="#58a6ff" font-size="12" text-anchor="middle">In-Memory JIT (SPIR-V)</text>
  
  <rect x="315" y="200" width="170" height="40" rx="6" fill="#21262d" stroke="#58a6ff"/>
  <text x="400" y="225" fill="#58a6ff" font-size="12" text-anchor="middle">IOSurface Texture SHM</text>
  
  <rect x="315" y="255" width="170" height="25" rx="4" fill="#21262d" stroke="#58a6ff"/>
  <text x="400" y="272" fill="#58a6ff" font-size="11" text-anchor="middle">Independent Swapchain</text>

  <!-- Connectors Middle to Right -->
  <path d="M 500 180 L 570 180" stroke="#ff5555" stroke-width="3" fill="none"/>
  
  <!-- Native Hardware Layer -->
  <rect x="570" y="110" width="200" height="140" rx="12" fill="#21262d" stroke="#ff5555" stroke-width="3"/>
  <text x="670" y="160" fill="#ff5555" font-size="18" font-weight="bold" text-anchor="middle">Vulkan 1.3</text>
  <text x="670" y="195" fill="#8b949e" font-size="12" text-anchor="middle">Standard PC Hardware</text>
  <text x="670" y="215" fill="#8b949e" font-size="11" text-anchor="middle">(NVIDIA / AMD / Intel)</text>
</svg>
</p>

---

## 🇬🇧 English

### Overview
**vulkan-cocoa-core** is a high-performance, hardware-independent graphics pipeline bridge designed to translate and redirect Apple's Cocoa, CoreAnimation, and Metal 3 interfaces directly into native Vulkan 1.3 API calls at the Darwin/XNU kernel level, replacing the proprietary Apple WindowServer on standard PC hardware.

### 🛠️ Key Technological Systems

| System Component | Technology Stack | Core Functionality |
| :--- | :--- | :--- |
| **Hardware-Independent Swapchain** | `Vulkan Queues`, `VRAM` | Dynamically manages video memory, allocating buffers in `DEVICE_LOCAL` VRAM for smooth 4K rendering. |
| **Multi-Process Texture Sharing** | `IOSurface API`, `POSIX shm`, `VK_KHR_external_memory_fd` | Enables zero-stall texture streaming between sandboxed browser processes and the GPU compositor. |
| **Kqueue/Mach Native Event Loop** | `Kernel Ports`, `Objective-C Runtime` | Asynchronous dispatcher translating input signals into `NSEvent` structures for full UI interactivity. |
| **In-Memory JIT Shader Compiler** | `LLVM`, `SPIR-V` | Intercepts Apple AIR bytecode, converts it to SPIR-V, injects `NonUniformEXT`, and converts memory barriers. |

---

## 🇷🇺 Русский

### Обзор проекта
**vulkan-cocoa-core** — это высокопроизводительный, аппаратно-независимый графический мост, разработанный для трансляции и перенаправления интерфейсов Apple Cocoa, CoreAnimation и Metal 3 напрямую в нативные вызовы Vulkan 1.3 API на уровне ядра Darwin/XNU. Проект заменяет проприетарный Apple WindowServer на стандартном ПК-оборудовании.

### 🛠️ Ключевые технологические системы

| Компонент системы | Технологический стек | Основной функционал |
| :--- | :--- | :--- |
| **Аппаратно-независимый Swapchain** | `Очереди Vulkan`, `VRAM` | Динамически управляет видеопамятью, выделяя буферы в логах `DEVICE_LOCAL` VRAM для плавного рендерингу в 4K. |
| **Мультипроцессорный обмен текстурами** | `IOSurface API`, `POSIX shm`, `VK_KHR_external_memory_fd` | Обеспечивает потоковую передачу текстур без задержек между изолированными процессами браузера и GPU-композитором. |
| **Нативный цикл событий Kqueue/Mach** | `Порты ядра`, `Objective-C Runtime` | Асинхронный диспетчер, транслирующий входные сигналы в структуры `NSEvent` для полного интерактивного взаимодействия с UI. |
| **JIT-компилятор шейдеров в памяти** | `LLVM`, `SPIR-V` | Перехватывает байт-код Apple AIR, преобразует его в SPIR-V, внедряет `NonUniformEXT` и конвертирует барьеры памяти. |

---

## 🇺🇦 Українська

### Огляд проекту
**vulkan-cocoa-core** — це високопродуктивний, апаратно-незалежний графічний міст, розроблений для трансляції та перенаправлення інтерфейсів Apple Cocoa, CoreAnimation та Metal 3 безпосередньо в нативні виклики Vulkan 1.3 API на рівні ядра Darwin/XNU. Проект замінює пропрієтарний Apple WindowServer на стандартному обладнанні ПК.

### 🛠️ Ключові технологічні системи

| Компонент системи | Технологічний стек | Основной функціонал |
| :--- | :--- | :--- |
| **Апаратно-незалежний Swapchain** | `Черги Vulkan`, `VRAM` | Динамічно керує відеопам'яттю, виділяючи буфери в `DEVICE_LOCAL` VRAM для плавного рендерингу в 4K. |
| **Мультипроцесорний обмін текстурами** | `IOSurface API`, `POSIX shm`, `VK_KHR_external_memory_fd` | Забезпечує потокову передачу текстур без затримок між ізольованими процесами браузера та GPU-композитором. |
| **Нативний цикл подій Kqueue/Mach** | `Порти ядра`, `Objective-C Runtime` | Асинхронний диспетчер, що транслює вхідні сигнали в структури `NSEvent` для повної інтерактивності UI. |
| **JIT-компілятор шейдерів у пам'яті** | `LLVM`, `SPIR-V` | Перехоплює байт-код Apple AIR, перетворює його на SPIR-V, впроваджує `NonUniformEXT` та конвертує бар'єри пам'яті. |

---

## 🇧🇾 Беларуская

### Агляд праекта
**vulkan-cocoa-core** — гэта высокапрадукцыйны, апаратна-незалежны графічны мост, распрацаваны для трансляцыі і перакіравання інтэрфейсаў Apple Cocoa, CoreAnimation і Metal 3 наўпрост у натыўныя выклікі Vulkan 1.3 API на ўзроўні ядра Darwin/XNU. Праект замяняе прапрыятаPaper Apple WindowServer на стандартным абсталяванні ПК.

### 🛠️ Ключавыя тэхналагічныя сістэмы

| Кампанент сістэмы | Тэхналагічны стэк | Асноўны функцыянал |
| :--- | :--- | :--- |
| **Апаратна-незалежны Swapchain** | `Чэргі Vulkan`, `VRAM` | Дынамічно кіруе відэапамяццю, вылучаючы буферы ў `DEVICE_LOCAL` VRAM для плаўнага рэндэрынгу ў 4K. |
| **Мультыпрэцэсарны абмен тэкстурамі** | `IOSurface API`, `POSIX shm`, `VK_KHR_external_memory_fd` | Забяспечвае патокавую перадачу тэкстур без затрымак паміж ізаляванымі працэсамі браўзера і GPU-кампазітарам. |
| **Натыўны цыкл падзей Kqueue/Mach** | `Парты ядра`, `Objective-C Runtime` | Асінхронны дыспетчар, які транслюе ўваходныя сігналы ў структуры `NSEvent` для поўнай інтэрактыўнасці UI. |
| **JIT-кампілятар шэйдараў у памяці** | `LLVM`, `SPIR-V` | Перахоплівае байт-код Apple AIR, пераўтварае яго ў SPIR-V, укараняе `NonUniformEXT` і канвертуе бар'еры памяці. |

---

## 🇮🇹 Italiano

### Panoramica
**vulkan-cocoa-core** è un bridge per pipeline grafica ad alte prestazioni e indipendente dall'hardware, progettato per tradurre e reindirizzare le interfacce Cocoa, CoreAnimation e Metal 3 di Apple direttamente in chiamate API native Vulkan 1.3 a livello di kernel Darwin/XNU, sostituendo l'Apple WindowServer proprietario su hardware PC standard.

### 🛠️ Sistemi Tecnologici Chiave

| Componente del Sistema | Stack Tecnologico | Funzionalità Principale |
| :--- | :--- | :--- |
| **Swapchain Indipendente dall'Hardware** | `Code Vulkan`, `VRAM` | Gestisce dinamicamente la memoria video, allocando i buffer in `DEVICE_LOCAL` VRAM per un rendering 4K fluido. |
| **Condivisione Texture Multi-Processo** | `IOSurface API`, `POSIX shm`, `VK_KHR_external_memory_fd` | Consente lo streaming di texture senza blocchi tra i processi sandbox del browser e il compositore GPU. |
| **Loop di Eventi Nativo Kqueue/Mach** | `Porte del Kernel`, `Objective-C Runtime` | Dispatcher asincrono che traduce i segnali di input in strutture `NSEvent` per una completa interattività della UI. |
| **Compilatore Shader JIT in Memoria** | `LLVM`, `SPIR-V` | Intercetta il bytecode Apple AIR, lo converte in SPIR-V, inietta `NonUniformEXT` e converte le barriere di memoria. |

---

## 📋 Architectural Requirements / Требования к системе

| Requirement | Specification |
| :--- | :--- |
| **OS Core** | Darwin/XNU Kernel OR Compatible Linux POSIX Layer |
| **Vulkan API** | Version 1.3+ with `VK_KHR_external_memory_fd` support |
| **Compiler** | Rust LLVM Toolchain (Nightly preferred) & Clang C/C++ |
