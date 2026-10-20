#[allow(dead_code)]
mod armazenamento;
mod comandos;
#[allow(dead_code)]
mod erro;
mod estado;
#[allow(dead_code)]
mod migracoes;
#[allow(dead_code)]
mod regras;
#[allow(dead_code)]
mod repositorio;

use chrono::Local;
use estado::Estado;
use std::path::PathBuf;
use tauri::{Manager, RunEvent, WindowEvent};

/// Pasta da aplicação: `FROTA_RAIZ` ou a pasta do executável (modo portátil).
fn raiz() -> PathBuf {
    if let Some(r) = std::env::var_os("FROTA_RAIZ").filter(|r| !r.is_empty()) {
        return PathBuf::from(r);
    }
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// `%LOCALAPPDATA%\Frota` (cópia de trabalho e marcador `pendente`).
fn local() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Frota")
}

fn nome_pc() -> String {
    std::env::var("COMPUTERNAME")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "desconhecido".into())
}

/// Grava, faz a cópia diária e liberta o lock (só atua uma vez).
fn fechar(app: &tauri::AppHandle) {
    if let Some(e) = app.try_state::<Estado>() {
        let agora = Local::now();
        if let Err(err) = e.fechar(agora.date_naive(), agora.naive_local()) {
            log::error!("erro ao fechar: {}", err.mensagem);
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        // Tem de ser o primeiro plugin: uma segunda instância copiaria por cima do
        // trabalho.db aberto. Em vez disso, foca a janela que já existe.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            let (estado, arranque) =
                Estado::arrancar(&raiz(), &local(), &nome_pc(), Local::now().naive_local());
            log::info!("arranque: {arranque:?}");
            app.manage(estado);
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
                handle.state::<Estado>().tick(Local::now().naive_local());
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            comandos::arranque,
            comandos::abrir_consulta,
            comandos::estado_armazenamento,
            comandos::fazer_copia,
            comandos::listar_versoes,
            comandos::restaurar_versao,
            comandos::carros_listar,
            comandos::carro_obter,
            comandos::carro_guardar,
            comandos::carro_apagar,
            comandos::motoristas_listar,
            comandos::motorista_guardar,
            comandos::motorista_apagar,
            comandos::atribuir,
            comandos::atribuicoes_listar,
            comandos::lancar_manutencao,
            comandos::lancar_pneu,
            comandos::lancar_documento,
            comandos::guardar_multa,
            comandos::condutor_sugerido,
            comandos::marcar_reembolsado,
            comandos::identificar_condutor,
            comandos::multas_motorista,
            comandos::oficinas_usadas,
            comandos::atualizar_km,
            comandos::pagar_documento,
            comandos::apagar_lancamento,
            comandos::multas_listar,
            comandos::historico,
            comandos::tipos_listar,
            comandos::tipo_guardar,
            comandos::planos_listar,
            comandos::plano_guardar,
            comandos::plano_apagar,
            comandos::limiares_obter,
            comandos::limiares_guardar,
            comandos::painel,
            comandos::relatorio,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app, event| match event {
        // Síncrono, quando a janela é destruída. Não em `CloseRequested`: o frontend escuta
        // `onCloseRequested` (para correr os apagamentos pendentes do "Desfazer"), o que faz
        // o Tauri impedir o fecho e só destruir a janela depois de o handler JS terminar —
        // fechar os dados em `CloseRequested` fá-los-ia falhar.
        RunEvent::WindowEvent {
            event: WindowEvent::Destroyed,
            ..
        } => fechar(app),
        // Vigilante: se o handler JS nunca terminar (página bloqueada), destrói a janela
        // ao fim de 5 s — `Destroyed` corre então `fechar` normalmente.
        RunEvent::WindowEvent {
            label,
            event: WindowEvent::CloseRequested { .. },
            ..
        } => {
            let app = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(5));
                if let Some(w) = app.get_webview_window(&label) {
                    log::warn!("fecho: o frontend não respondeu em 5 s; a destruir a janela");
                    if let Err(e) = w.destroy() {
                        log::error!("fecho: não foi possível destruir a janela: {e}");
                    }
                }
            });
        }
        // Saída por outro caminho (p.ex. última janela destruída): `fechar` só atua uma vez.
        RunEvent::ExitRequested { .. } | RunEvent::Exit => fechar(app),
        _ => {}
    });
}

#[cfg(test)]
mod testes {
    #[test]
    fn fumo() {
        let e: crate::erro::Erro = std::io::Error::other("x").into();
        assert_eq!(e.codigo, "io");
    }
}
