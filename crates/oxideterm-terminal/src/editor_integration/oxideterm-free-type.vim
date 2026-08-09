" OxideTerm Free Type Mode integration for Vim and Neovim.
" Source this file from vimrc/init.vim only for terminal sessions where the
" editor should expose explicit mode and selection state to OxideTerm.

if exists('g:loaded_oxideterm_free_type')
  finish
endif
let g:loaded_oxideterm_free_type = 1

let s:application = has('nvim') ? 'nvim' : 'vim'
let s:last_state = ''
let s:heartbeat_timer = -1

function! s:write_terminal(payload) abort
  " A shared multiplexer pane cannot route private OSC to one attached client.
  let l:term_program = tolower(exists('$TERM_PROGRAM') ? $TERM_PROGRAM : '')
  let l:lc_terminal = tolower(exists('$LC_TERMINAL') ? $LC_TERMINAL : '')
  let l:private_osc = exists('$OXIDETERM_PRIVATE_OSC') ? $OXIDETERM_PRIVATE_OSC : ''
  let l:shared_multiplexer = (exists('$TMUX') && !empty($TMUX))
        \ || (exists('$STY') && !empty($STY))
        \ || (exists('$ZELLIJ') && !empty($ZELLIJ))
  if l:shared_multiplexer
        \ || (l:private_osc !=# '1' && l:term_program !=# 'oxideterm' && l:lc_terminal !=# 'oxideterm')
    return
  endif
  try
    if has('nvim')
      call chansend(v:stderr, a:payload)
    elseif has('unix') && filewritable('/dev/tty') == 1
      call writefile([a:payload], '/dev/tty', 'b')
    endif
  catch
    " Integration metadata must never interrupt editor input.
  endtry
endfunction

function! s:mode_name() abort
  let l:raw = mode(1)
  if l:raw =~# '^[i]'
    return 'insert'
  elseif l:raw =~# '^[R]'
    return 'replace'
  elseif l:raw =~# '^[vV\x16]'
    return 'visual'
  elseif l:raw =~# '^[sS\x13]'
    return 'select'
  endif
  return 'normal'
endfunction

function! s:selection_name() abort
  let l:raw = mode(1)
  if l:raw =~# '^[vVsS]'
    return l:raw[0] ==# 'V' || l:raw[0] ==# 'S' ? 'line' : 'char'
  elseif l:raw =~# '^[\x16\x13]'
    return 'block'
  endif
  return 'none'
endfunction

function! s:state_payload(active) abort
  return printf(
        \ "\033]7719;v=3;kind=editor-state;app=%s;mode=%s;selection=%s;caps=mouse,clipboard,edit;active=%d\007",
        \ s:application,
        \ s:mode_name(),
        \ s:selection_name(),
        \ a:active)
endfunction

function! s:emit_state(force) abort
  let l:payload = s:state_payload(1)
  if a:force || l:payload !=# s:last_state
    let s:last_state = l:payload
    call s:write_terminal(l:payload)
  endif
endfunction

function! s:heartbeat(timer) abort
  call s:emit_state(1)
endfunction

function! s:percent_encode(text) abort
  return join(map(str2list(a:text), {_, byte -> printf('%%%02X', byte)}), '')
endfunction

function! s:emit_clipboard(operation, text) abort
  let l:payload = printf(
        \ "\033]7719;v=3;kind=editor-clipboard;app=%s;op=%s;data=%s\007",
        \ s:application,
        \ a:operation,
        \ s:percent_encode(a:text))
  call s:write_terminal(l:payload)
endfunction

function! s:visual_text(restore_visual) abort
  if !a:restore_visual
    return ''
  endif
  silent normal! gv
  let l:saved_register = getreg('"')
  let l:saved_type = getregtype('"')
  silent normal! y
  let l:text = getreg('"')
  call setreg('"', l:saved_register, l:saved_type)
  silent normal! gv
  return l:text
endfunction

function! s:copy_selection(restore_visual) abort
  let l:text = s:visual_text(a:restore_visual)
  if !empty(l:text)
    call s:emit_clipboard('copy', l:text)
  endif
  call s:emit_state(1)
endfunction

function! s:cut_selection(restore_visual) abort
  let l:text = s:visual_text(a:restore_visual)
  if !empty(l:text)
    silent normal! d
    call s:emit_clipboard('cut', l:text)
  endif
  call s:emit_state(1)
endfunction

function! s:prepare_paste(restore_visual) abort
  if a:restore_visual
    silent normal! gv
    silent normal! c
  elseif s:mode_name() ==# 'normal'
    startinsert
  endif
  call s:emit_state(1)
endfunction

function! s:delete_selection(restore_visual) abort
  if a:restore_visual
    silent normal! gv
    silent normal! d
  endif
  call s:emit_state(1)
endfunction

function! s:leave() abort
  if s:heartbeat_timer != -1
    call timer_stop(s:heartbeat_timer)
    let s:heartbeat_timer = -1
  endif
  call s:write_terminal(s:state_payload(0))
endfunction

" These private CSI sequences are emitted only after a current adapter
" heartbeat, so they do not claim physical function keys from user mappings.
execute "set <F13>=\<Esc>[99;1~"
execute "set <F14>=\<Esc>[99;2~"
execute "set <F15>=\<Esc>[99;3~"
execute "set <F16>=\<Esc>[99;4~"
nnoremap <silent> <F13> :call <SID>copy_selection(0)<CR>
xnoremap <silent> <F13> :<C-U>call <SID>copy_selection(1)<CR>
nnoremap <silent> <F14> :call <SID>cut_selection(0)<CR>
xnoremap <silent> <F14> :<C-U>call <SID>cut_selection(1)<CR>
nnoremap <silent> <F15> :call <SID>prepare_paste(0)<CR>
xnoremap <silent> <F15> :<C-U>call <SID>prepare_paste(1)<CR>
inoremap <silent> <F15> <C-O>:call <SID>prepare_paste(0)<CR>
nnoremap <silent> <F16> :call <SID>delete_selection(0)<CR>
xnoremap <silent> <F16> :<C-U>call <SID>delete_selection(1)<CR>

set mouse=a
augroup oxideterm_free_type
  autocmd!
  autocmd VimEnter,ModeChanged,CursorMoved,CursorMovedI * call <SID>emit_state(0)
  autocmd VimLeavePre * call <SID>leave()
augroup END

if exists('*timer_start')
  let s:heartbeat_timer = timer_start(1000, function('<SID>heartbeat'), {'repeat': -1})
endif
call s:emit_state(1)
