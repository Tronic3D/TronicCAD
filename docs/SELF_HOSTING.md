# TronicCAD-Teamserver selbst betreiben

Für Unternehmen, die Projekte auf eigener Hardware halten wollen. Die Anwendung selbst braucht keinen Server: Dies ist nur für Teamarbeit und zentrale Speicherung nötig.

**Stand:** Beide Wege unten wurden wörtlich auf einem frischen Ubuntu 26.04 ausgeführt, siehe „Geprüft“ am Ende. Andere Systeme sind nicht geprüft. Ein Dauerbetrieb mit echten Nutzern hat nicht stattgefunden, und **eine unabhängige Sicherheitsprüfung steht aus** ([SECURITY.md](../SECURITY.md)).

## Was Sie brauchen

- Einen Linux-Rechner mit Administratorrechten, mindestens 2 GB Arbeitsspeicher und genug Platz für alle Projektversionen. Versionen werden nie automatisch gelöscht.
- Das Quellpaket `troniccad-source.tar.gz` im Heimatverzeichnis von root. Fertige Programmpakete gibt es noch nicht, der Dienst wird daher bei der Installation gebaut.
- Internetzugang während der Installation.
- Für einen Betrieb im Netz: einen Rechnernamen, den Ihre Teammitglieder erreichen. Die Beispiele verwenden `localhost`, was nur auf dem Server selbst funktioniert.

Der Dienst selbst spricht kein TLS. In beiden Wegen übernimmt das der vorgeschaltete Proxy Caddy.

## Weg A: Docker

Docker installieren:

<!-- selfhost:docker -->
```bash
apt-get update
apt-get install -y docker.io docker-compose-v2 ca-certificates curl openssl
systemctl enable --now docker
```

Quellpaket auspacken und Passwort für die Datenbank erzeugen:

<!-- selfhost:docker -->
```bash
mkdir -p /opt/troniccad
tar xzf ~/troniccad-source.tar.gz -C /opt/troniccad
cd /opt/troniccad/services/cloud/selfhost
cp env.example .env
sed -i "s/^POSTGRES_PASSWORD=.*/POSTGRES_PASSWORD=$(openssl rand -hex 24)/" .env
```

In `.env` tragen Sie unter `TRONIC_HOST` den Rechnernamen ein. Dann bauen und starten. Das dauert beim ersten Mal mehrere Minuten:

<!-- selfhost:docker -->
```bash
cd /opt/troniccad/services/cloud/selfhost
docker compose up -d --build
```

Das erste Konto anlegen. Die Registrierung ist geschlossen: Weitere Personen kommen nur über Einladungen hinein. Das Passwort braucht mindestens 12 Zeichen:

<!-- selfhost:docker -->
```bash
cd /opt/troniccad/services/cloud/selfhost
export TRONIC_NEW_PASSWORD="$(openssl rand -hex 16)"
echo "$TRONIC_NEW_PASSWORD" > /root/admin-password.txt
chmod 600 /root/admin-password.txt
docker compose exec -T -e TRONIC_NEW_PASSWORD app tronic-cloud create-account admin@example.com
```

Prüfen, ob alles läuft:

<!-- selfhost:docker -->
```bash
cd /opt/troniccad/services/cloud/selfhost
for i in $(seq 1 60); do docker compose cp caddy:/data/caddy/pki/authorities/local/root.crt /root/troniccad-ca.crt 2>/dev/null && break; sleep 2; done
for i in $(seq 1 60); do curl --silent --fail --cacert /root/troniccad-ca.crt https://localhost/healthz && break; sleep 2; done
curl --silent --show-error --fail --cacert /root/troniccad-ca.crt https://localhost/healthz
curl --silent --show-error --fail --cacert /root/troniccad-ca.crt -H 'content-type: application/json' \
  -d "{\"email\":\"admin@example.com\",\"password\":\"$(cat /root/admin-password.txt)\"}" \
  https://localhost/v1/auth/login | grep -q token
test "$(curl --silent --cacert /root/troniccad-ca.crt -o /dev/null -w '%{http_code}' -H 'content-type: application/json' \
  -d '{"email":"someone@example.com","password":"a long enough password"}' https://localhost/v1/auth/register)" = 403
TOKEN="$(curl --silent --show-error --fail --cacert /root/troniccad-ca.crt -H 'content-type: application/json' \
  -d "{\"email\":\"admin@example.com\",\"password\":\"$(cat /root/admin-password.txt)\"}" \
  https://localhost/v1/auth/login | sed -n 's/.*"token":"\([^"]*\)".*/\1/p')"
date > /root/check-object.txt
KEY="projects/$(cat /proc/sys/kernel/random/uuid)/blobs/$(sha256sum /root/check-object.txt | cut -d' ' -f1)"
curl --silent --show-error --fail --cacert /root/troniccad-ca.crt -X PUT -H "authorization: Bearer $TOKEN" \
  --data-binary @/root/check-object.txt "https://localhost/v1/objects/$KEY"
echo "$KEY" > /root/check-object-key.txt
echo "server is working"
```

Ein Neustart übersteht alle Daten, auch die abgelegten Objekte:

<!-- selfhost:docker -->
```bash
cd /opt/troniccad/services/cloud/selfhost
docker compose restart
for i in $(seq 1 60); do curl --silent --fail --cacert /root/troniccad-ca.crt https://localhost/healthz && break; sleep 2; done
curl --silent --show-error --fail --cacert /root/troniccad-ca.crt -H 'content-type: application/json' \
  -d "{\"email\":\"admin@example.com\",\"password\":\"$(cat /root/admin-password.txt)\"}" \
  https://localhost/v1/auth/login | sed -n 's/.*"token":"\([^"]*\)".*/\1/p' > /root/check-token.txt
curl --silent --show-error --fail --cacert /root/troniccad-ca.crt -H "authorization: Bearer $(cat /root/check-token.txt)" \
  "https://localhost/v1/objects/$(cat /root/check-object-key.txt)" | cmp - /root/check-object.txt
echo "data survived the restart"
```

Sichern und aus der Sicherung wiederherstellen. **Beides gehört zusammen:** Datenbank und Objektspeicher, in dieser Reihenfolge:

<!-- selfhost:docker -->
```bash
cd /opt/troniccad/services/cloud/selfhost
mkdir -p /root/backup
docker compose exec -T db pg_dump --format=custom -U troniccad troniccad > /root/backup/troniccad.dump
docker compose exec -T app tar czf - -C /data store > /root/backup/store.tar.gz
docker compose stop app
docker compose exec -T db dropdb -U troniccad troniccad
docker compose run --rm -T --no-deps --user root --entrypoint sh app -c 'rm -rf /data/store/*'
docker compose exec -T db createdb -U troniccad troniccad
docker compose exec -T db pg_restore -U troniccad --dbname=troniccad --no-owner < /root/backup/troniccad.dump
docker compose run --rm -T --no-deps --user root --entrypoint sh app -c 'tar xzf - -C /data && chown -R troniccad:troniccad /data/store' < /root/backup/store.tar.gz
docker compose start app
for i in $(seq 1 60); do curl --silent --fail --cacert /root/troniccad-ca.crt https://localhost/healthz && break; sleep 2; done
curl --silent --show-error --fail --cacert /root/troniccad-ca.crt -H 'content-type: application/json' \
  -d "{\"email\":\"admin@example.com\",\"password\":\"$(cat /root/admin-password.txt)\"}" \
  https://localhost/v1/auth/login | sed -n 's/.*"token":"\([^"]*\)".*/\1/p' > /root/check-token.txt
curl --silent --show-error --fail --cacert /root/troniccad-ca.crt -H "authorization: Bearer $(cat /root/check-token.txt)" \
  "https://localhost/v1/objects/$(cat /root/check-object-key.txt)" | cmp - /root/check-object.txt
echo "restored from the backup"
```

## Weg B: Ohne Docker

Pakete installieren und Rust einrichten. Unter `/etc/os-release` sollte ein aktuelles Debian oder Ubuntu stehen:

<!-- selfhost:native -->
```bash
apt-get update
apt-get install -y postgresql caddy build-essential pkg-config ca-certificates curl openssl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
. "$HOME/.cargo/env"
```

Quellpaket auspacken und den Dienst bauen. Das dauert einige Minuten:

<!-- selfhost:native -->
```bash
. "$HOME/.cargo/env"
mkdir -p /opt/troniccad
tar xzf ~/troniccad-source.tar.gz -C /opt/troniccad
cd /opt/troniccad
cargo build --release --locked -p tronic-cloud --bin tronic-cloud --example check_server
install -m 755 target/release/tronic-cloud /usr/local/bin/tronic-cloud
```

Einen eigenen Benutzer, die Datenbank und die Einstellungen anlegen:

<!-- selfhost:native -->
```bash
useradd --system --home-dir /var/lib/troniccad --create-home --shell /usr/sbin/nologin troniccad
install -d -o troniccad -g troniccad -m 700 /var/lib/troniccad/store
DB_PASSWORD="$(openssl rand -hex 24)"
sudo -u postgres psql -v ON_ERROR_STOP=1 -c "CREATE ROLE troniccad LOGIN PASSWORD '$DB_PASSWORD';"
sudo -u postgres psql -v ON_ERROR_STOP=1 -c "CREATE DATABASE troniccad OWNER troniccad;"
cat > /etc/troniccad.env <<EOF
DATABASE_URL=postgres://troniccad:$DB_PASSWORD@127.0.0.1:5432/troniccad
TRONIC_CLOUD_BIND=127.0.0.1:8787
TRONIC_STORE_DIR=/var/lib/troniccad/store
TRONIC_ALLOW_REGISTRATION=false
EOF
chmod 600 /etc/troniccad.env
```

Als Systemdienst einrichten und starten:

<!-- selfhost:native -->
```bash
install -m 644 /opt/troniccad/services/cloud/selfhost/troniccad.service /etc/systemd/system/troniccad.service
systemctl daemon-reload
systemctl enable --now troniccad
```

Caddy vorschalten. Tragen Sie statt `localhost` den Rechnernamen ein. Bei einem öffentlich erreichbaren Namen holt Caddy selbst ein Zertifikat, sonst gilt die interne Zertifizierungsstelle:

<!-- selfhost:native -->
```bash
cat > /etc/caddy/Caddyfile <<'EOF'
localhost {
    tls internal
    reverse_proxy 127.0.0.1:8787
}
EOF
systemctl restart caddy
```

Das erste Konto anlegen:

<!-- selfhost:native -->
```bash
set -a; . /etc/troniccad.env; set +a
export TRONIC_NEW_PASSWORD="$(openssl rand -hex 16)"
echo "$TRONIC_NEW_PASSWORD" > /root/admin-password.txt
chmod 600 /root/admin-password.txt
tronic-cloud create-account admin@example.com
```

Der internen Zertifizierungsstelle vertrauen und alles prüfen. Dasselbe Zertifikat müssen die Rechner Ihres Teams in ihren Zertifikatspeicher aufnehmen, die Anwendung verwendet den des Betriebssystems. Die Prüfung arbeitet wie die Anwendung: mit zwei Geräten, einem echten Projekt, Sperren und Audit-Protokoll. Sie notiert außerdem, was danach gespeichert ist:

<!-- selfhost:native -->
```bash
for i in $(seq 1 30); do test -f /var/lib/caddy/.local/share/caddy/pki/authorities/local/root.crt && break; curl --silent --insecure https://localhost/healthz > /dev/null || true; sleep 1; done
install -m 644 /var/lib/caddy/.local/share/caddy/pki/authorities/local/root.crt /usr/local/share/ca-certificates/troniccad-internal.crt
update-ca-certificates
for i in $(seq 1 60); do curl --silent --fail https://localhost/healthz && break; sleep 1; done
curl --silent --show-error --fail https://localhost/healthz
TRONIC_CHECK_PASSWORD="$(cat /root/admin-password.txt)" /opt/troniccad/target/release/examples/check_server https://localhost admin@example.com --save /root/check-state.json
```

Sichern und aus der Sicherung wiederherstellen. **Beides gehört zusammen:** Datenbank und Objektspeicher. Die Prüfung vergleicht danach Versionen und Audit-Protokoll mit dem Notierten:

<!-- selfhost:native -->
```bash
mkdir -p /root/backup
sudo -u postgres pg_dump --format=custom troniccad > /root/backup/troniccad.dump
tar czf /root/backup/store.tar.gz -C /var/lib/troniccad store
systemctl stop troniccad
sudo -u postgres dropdb troniccad
rm -rf /var/lib/troniccad/store
sudo -u postgres createdb --owner=troniccad troniccad
sudo -u postgres pg_restore --dbname=troniccad --no-owner --role=troniccad < /root/backup/troniccad.dump
tar xzf /root/backup/store.tar.gz -C /var/lib/troniccad
chown -R troniccad:troniccad /var/lib/troniccad/store
systemctl start troniccad
for i in $(seq 1 60); do curl --silent --fail https://localhost/healthz && break; sleep 1; done
TRONIC_CHECK_PASSWORD="$(cat /root/admin-password.txt)" /opt/troniccad/target/release/examples/check_server https://localhost admin@example.com --compare /root/check-state.json
echo "restored from the backup"
```

## Die Anwendung verbinden

In TronicCAD unter „Cloud“ einen Speicher der Art „TronicCAD-Cloud“ hinzufügen: Adresse des Servers, E-Mail und Passwort. Unter „Team“ legen Sie dann das Team an und laden andere ein. Eingeladene Personen legen ihr Konto in denselben Feldern an, mit „Neues Konto anlegen“ und dem Einladungscode, denn die Registrierung ist geschlossen.

## Betrieb

- **Einstellungen:** siehe [docs/specs/cloud.md](specs/cloud.md).
- **Aktualisieren:** Neues Quellpaket auspacken, wie oben bauen und den Dienst neu starten. Das Schema passt sich beim Start an, wenn es sich geändert hat. Vorher sichern, und bei mehreren Instanzen vorher alle anhalten: Laufende Anfragen können sonst an der Änderung scheitern. Ein echter Wechsel zwischen zwei Fassungen ist nicht geprüft, weil es erst eine gibt.
- **Datenschutz:** Sie sind für die Daten verantwortlich. Das Audit-Protokoll enthält E-Mail-Adressen und bleibt auch nach einer Kontolöschung bestehen. Das ist mit Ihrer Rechtsabteilung zu klären.
- **Mehrere Instanzen:** Der Dienst hält keinen eigenen Zustand. Bei gemeinsamem PostgreSQL und gemeinsamem Objektspeicher, zum Beispiel S3, lassen sich mehrere Instanzen hinter einen Lastverteiler stellen. Ein Betrieb über mehrere Rechner ist nicht geprüft.
- **Datenbank:** Die Verbindung zur Datenbank ist unverschlüsselt und gehört auf denselben Rechner oder in ein privates Netz.

## Geprüft

`scripts/verify_selfhost.py` nimmt die markierten Befehle dieser Datei unverändert und führt sie in einer frischen Kopie von Ubuntu 26.04 unter WSL 2 aus, je Weg in einer neuen Kopie. Ergebnisse stehen in [reports/PERFORMANCE.md](../reports/PERFORMANCE.md).

Der Docker-Weg prüft Anmeldung, geschlossene Registrierung, Objekte, Neustart und Wiederherstellung. Team, Sperren und Zusammenführen prüft der Weg ohne Docker mit demselben Programm.

Nicht geprüft: ein echter Server statt WSL, andere Distributionen, öffentliche Zertifikate über Let's Encrypt, Betrieb über Wochen.
