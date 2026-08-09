;;; oxideterm-free-type.el --- OxideTerm terminal editor integration -*- lexical-binding: t; -*-

;; This adapter reports explicit Emacs/Evil editing state and installs bounded
;; operation keys used only after OxideTerm receives a current heartbeat.

(defvar oxideterm-free-type--last-state nil)
(defvar oxideterm-free-type--heartbeat nil)

(defun oxideterm-free-type--private-osc-available-p ()
  "Return non-nil only when private OSC is isolated to an OxideTerm session."
  (let ((term-program (downcase (or (getenv "TERM_PROGRAM") "")))
        (lc-terminal (downcase (or (getenv "LC_TERMINAL") ""))))
    (and (= (length (or (getenv "TMUX") "")) 0)
         (= (length (or (getenv "STY") "")) 0)
         (= (length (or (getenv "ZELLIJ") "")) 0)
         (or (equal (getenv "OXIDETERM_PRIVATE_OSC") "1")
             (equal term-program "oxideterm")
             (equal lc-terminal "oxideterm")))))

(defun oxideterm-free-type--write-terminal (payload)
  "Write PAYLOAD only across an isolated OxideTerm terminal boundary."
  (when (oxideterm-free-type--private-osc-available-p)
    (send-string-to-terminal payload)))

(defun oxideterm-free-type--mode-name ()
  (if (bound-and-true-p evil-local-mode)
      (pcase evil-state
        ('normal "normal")
        ('insert "insert")
        ('replace "replace")
        ('visual "visual")
        (_ "emacs"))
    "emacs"))

(defun oxideterm-free-type--selection-name ()
  (if (use-region-p) "region" "none"))

(defun oxideterm-free-type--state-payload (active)
  (format "\e]7719;v=3;kind=editor-state;app=emacs;mode=%s;selection=%s;caps=mouse,clipboard,edit;active=%d\a"
          (oxideterm-free-type--mode-name)
          (oxideterm-free-type--selection-name)
          (if active 1 0)))

(defun oxideterm-free-type--emit-state (&optional force)
  (let ((payload (oxideterm-free-type--state-payload t)))
    (when (or force (not (equal payload oxideterm-free-type--last-state)))
      (setq oxideterm-free-type--last-state payload)
      (oxideterm-free-type--write-terminal payload))))

(defun oxideterm-free-type--percent-encode (text)
  ;; Encode every UTF-8 byte so the OSC field grammar never depends on URI
  ;; libraries choosing which printable characters to preserve.
  (mapconcat (lambda (byte) (format "%%%02X" byte))
             (string-to-list (encode-coding-string text 'utf-8))
             ""))

(defun oxideterm-free-type--emit-clipboard (operation text)
  (oxideterm-free-type--write-terminal
   (format "\e]7719;v=3;kind=editor-clipboard;app=emacs;op=%s;data=%s\a"
           operation
           (oxideterm-free-type--percent-encode text))))

(defun oxideterm-free-type-copy-selection ()
  (interactive)
  (when (use-region-p)
    (let ((text (buffer-substring-no-properties
                 (region-beginning) (region-end))))
      (kill-new text)
      (oxideterm-free-type--emit-clipboard "copy" text)))
  (oxideterm-free-type--emit-state t))

(defun oxideterm-free-type-cut-selection ()
  (interactive)
  (when (use-region-p)
    (let ((text (buffer-substring-no-properties
                 (region-beginning) (region-end))))
      (kill-region (region-beginning) (region-end))
      (oxideterm-free-type--emit-clipboard "cut" text)))
  (oxideterm-free-type--emit-state t))

(defun oxideterm-free-type-prepare-paste ()
  (interactive)
  (when (use-region-p)
    (delete-region (region-beginning) (region-end)))
  (oxideterm-free-type--emit-state t))

(defun oxideterm-free-type-delete-selection ()
  (interactive)
  (when (use-region-p)
    (delete-region (region-beginning) (region-end)))
  (oxideterm-free-type--emit-state t))

(defvar oxideterm-free-type-mode-map
  (let ((map (make-sparse-keymap)))
    (define-key map [oxideterm-free-type-copy]
                #'oxideterm-free-type-copy-selection)
    (define-key map [oxideterm-free-type-cut]
                #'oxideterm-free-type-cut-selection)
    (define-key map [oxideterm-free-type-paste]
                #'oxideterm-free-type-prepare-paste)
    (define-key map [oxideterm-free-type-delete]
                #'oxideterm-free-type-delete-selection)
    map))

;;;###autoload
(define-minor-mode oxideterm-free-type-mode
  "Expose explicit terminal editing state to OxideTerm Free Type Mode."
  :global t
  :keymap oxideterm-free-type-mode-map
  (if oxideterm-free-type-mode
      (progn
        (define-key input-decode-map "\e[99;5~" [oxideterm-free-type-copy])
        (define-key input-decode-map "\e[99;6~" [oxideterm-free-type-cut])
        (define-key input-decode-map "\e[99;7~" [oxideterm-free-type-paste])
        (define-key input-decode-map "\e[99;8~" [oxideterm-free-type-delete])
        (when (fboundp 'xterm-mouse-mode)
          (xterm-mouse-mode 1))
        (add-hook 'post-command-hook #'oxideterm-free-type--emit-state)
        (setq oxideterm-free-type--heartbeat
              (run-at-time 0 1 #'oxideterm-free-type--emit-state t)))
    (remove-hook 'post-command-hook #'oxideterm-free-type--emit-state)
    (when (timerp oxideterm-free-type--heartbeat)
      (cancel-timer oxideterm-free-type--heartbeat))
    (setq oxideterm-free-type--heartbeat nil)
    (oxideterm-free-type--write-terminal (oxideterm-free-type--state-payload nil))))

(add-hook 'kill-emacs-hook
          (lambda ()
            (when oxideterm-free-type-mode
              (oxideterm-free-type--write-terminal
               (oxideterm-free-type--state-payload nil)))))

(provide 'oxideterm-free-type)
;;; oxideterm-free-type.el ends here
