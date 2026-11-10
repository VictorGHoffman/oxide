fn main() {
    // Embute o ícone no executável (aparece no Explorer e na barra de tarefas)
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/oxide.ico");
        if let Err(e) = res.compile() {
            // Não derruba o build: a janela continua com ícone via código
            println!("cargo:warning=não foi possível embutir o ícone no .exe: {e}");
        }
    }
    println!("cargo:rerun-if-changed=assets/oxide.ico");
}
