#[allow(dead_code)]
mod armazenamento;
#[allow(dead_code)]
mod erro;
#[allow(dead_code)]
mod migracoes;
#[allow(dead_code)]
mod regras;
#[allow(dead_code)]
mod repositorio;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }
      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while building tauri application");
}

#[cfg(test)]
mod testes {
    #[test]
    fn fumo() {
        let e: crate::erro::Erro = std::io::Error::other("x").into();
        assert_eq!(e.codigo, "io");
    }
}


