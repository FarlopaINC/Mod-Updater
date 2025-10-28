# Mods Updater

Una aplicación para gestionar y actualizar mods de Minecraft desde Modrinth.

## Características

- Interfaz gráfica con egui
- Detección automática de mods instalados
- Actualización en lote de mods seleccionados
- Soporte para diferentes versiones de Minecraft
- Descarga paralela de actualizaciones

## Requisitos

- Rust (instalar desde [rustup](https://rustup.rs/))
- Git (instalar desde [git-scm.com](https://git-scm.com/download/win))

## Instalación

1. Instalar Rust:
   ```powershell
   # Descargar y ejecutar rustup-init.exe desde https://rustup.rs/
   ```

2. Clonar el repositorio:
   ```powershell
   git clone https://github.com/TU_USUARIO/mods-updater.git
   cd mods-updater
   ```

3. Compilar y ejecutar:
   ```powershell
   cargo run
   ```

## Uso

1. La aplicación detectará automáticamente los mods instalados en `.minecraft/mods`
2. Selecciona la versión de Minecraft objetivo en el desplegable superior
3. Marca los mods que deseas actualizar
4. Haz clic en "💾 Actualizar seleccionados". Estos mods se descargarán en UNA CARPETA APARTE (.minecraft/modpacks/mods1.x.y)
5. ¡Espera a que se complete la actualización!

## Licensed under either of

- Apache License, Version 2.0
- MIT license

