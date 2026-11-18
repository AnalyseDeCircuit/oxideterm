use super::*;
use oxideterm_gpui_connections::NewConnectionField;

impl WorkspaceApp {
    pub(in crate::workspace) fn ssh_tunnel_input_fields(&self) -> Option<Vec<NewConnectionField>> {
        let form = self.ssh_tunnels.form.as_ref()?;
        let mut fields = vec![NewConnectionField::Name];
        if form.saved.is_none() {
            fields.extend([NewConnectionField::Host, NewConnectionField::Port]);
            if form.kind != ForwardType::Dynamic {
                fields.extend([
                    NewConnectionField::TunnelTargetHost,
                    NewConnectionField::TunnelTargetPort,
                ]);
            }
        }
        Some(fields)
    }
}
