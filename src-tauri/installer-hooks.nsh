!macro NSIS_HOOK_POSTINSTALL
  nsExec::ExecToStack '"$INSTDIR\codebook-native-host.exe" --register "$INSTDIR"'
  Pop $0
  Pop $1
  ${If} $0 != 0
    MessageBox MB_OK|MB_ICONEXCLAMATION "Codebook 已安装，但浏览器连接注册失败。请参照 README 手动注册。"
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToStack '"$INSTDIR\codebook-native-host.exe" --unregister'
  Pop $0
  Pop $1
  ; Never touch the user's encrypted vault or backups during uninstall.
  Delete "$INSTDIR\codebook-native-host.json"
!macroend
