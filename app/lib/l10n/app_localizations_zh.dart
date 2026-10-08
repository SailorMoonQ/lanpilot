// ignore: unused_import
import 'package:intl/intl.dart' as intl;

import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for Chinese (`zh`).
class AppLocalizationsZh extends AppLocalizations {
  AppLocalizationsZh([String locale = 'zh']) : super(locale);

  @override
  String get appTitle => 'LanPilot';

  @override
  String get tabTouchpad => '触摸板';

  @override
  String get tabShortcuts => '快捷指令';

  @override
  String get tabMedia => '媒体';

  @override
  String get statusConnecting => '正在连接...';

  @override
  String get statusReconnecting => '正在重新连接...';

  @override
  String get statusOffline => '电脑离线，正在重试...';

  @override
  String get statusRemoved => '该电脑已移除此设备，请重新配对';

  @override
  String get statusUpdateComputer => '请更新电脑端 LanPilot';

  @override
  String get statusUpdateApp => '请更新 App';

  @override
  String get pairAgain => '重新配对';

  @override
  String get localNetworkTitle => '需要本地网络权限';

  @override
  String get localNetworkBody => '请在 设置 > 隐私与安全性 > 本地网络 中开启 LanPilot';

  @override
  String get openSettings => '打开设置';

  @override
  String get retry => '重试';

  @override
  String get switcherTitle => '电脑';

  @override
  String get online => '在线';

  @override
  String get offline => '离线';

  @override
  String get unpair => '取消配对';

  @override
  String unpairConfirm(String name) {
    return '取消与“$name”的配对？';
  }

  @override
  String get cancel => '取消';

  @override
  String get addComputer => '添加电脑';

  @override
  String get settings => '设置';

  @override
  String get addComputerHint => '在电脑上运行 LanPilot，扫描它显示的二维码，或粘贴配对链接';

  @override
  String get scanQr => '扫描二维码';

  @override
  String get scanHint => '将相机对准电脑上的二维码';

  @override
  String get pasteLink => '粘贴配对链接';

  @override
  String get paste => '粘贴';

  @override
  String get pair => '配对';

  @override
  String get pairing => '正在配对...';

  @override
  String get nearbyComputers => '附近的电脑';

  @override
  String get searching => '正在搜索...';

  @override
  String get manualIp => '手动输入 IP';

  @override
  String get address => '地址（IP 或 IP:端口）';

  @override
  String get password => '配对密码';

  @override
  String get pairWithPassword => '用密码配对';

  @override
  String get errInvalidCode => '这不是 LanPilot 配对链接';

  @override
  String get errUnreachable => '连不上这台电脑，请确认手机和电脑在同一网络';

  @override
  String get errWrongComputer => '应答的不是要配对的那台电脑';

  @override
  String get errCodeExpired => '配对码已失效，请在电脑上刷新后重试';

  @override
  String get errWrongPassword => '密码错误';

  @override
  String errLocked(int seconds) {
    return '尝试次数过多，请 $seconds 秒后再试';
  }

  @override
  String get errPasswordDisabled => '这台电脑未开启密码配对';

  @override
  String get errDenied => '电脑拒绝了配对';

  @override
  String errGeneric(String message) {
    return '出错了：$message';
  }

  @override
  String get actionFailed => '操作失败，请检查连接';

  @override
  String get comingSoon => '即将推出';

  @override
  String get mediaPlayPause => '播放/暂停';

  @override
  String get mediaNext => '下一首';

  @override
  String get mediaPrevious => '上一首';

  @override
  String get mediaVolumeUp => '调高音量';

  @override
  String get mediaVolumeDown => '调低音量';

  @override
  String get mediaMute => '静音';

  @override
  String get buttonLeft => '左键';

  @override
  String get buttonRight => '右键';

  @override
  String get sectionTouchpad => '触摸板';

  @override
  String get sensitivity => '灵敏度';

  @override
  String get acceleration => '加速度';

  @override
  String get accelOff => '关';

  @override
  String get accelLow => '低';

  @override
  String get accelMedium => '中';

  @override
  String get accelHigh => '高';

  @override
  String get scrollSpeed => '滚动速度';

  @override
  String get naturalScroll => '自然滚动';

  @override
  String get tapDelay => '点击延迟';

  @override
  String tapDelayValue(int ms) {
    return '$ms 毫秒';
  }

  @override
  String get haptics => '振动反馈';

  @override
  String get showButtonBar => '显示左右键';

  @override
  String get sectionAppearance => '外观';

  @override
  String get theme => '主题';

  @override
  String get themeNative => '原生';

  @override
  String get themeDark => '深色沉浸';

  @override
  String get themeBrand => '品牌色';

  @override
  String get language => '语言';

  @override
  String get languageSystem => '跟随系统';

  @override
  String get languageZh => '中文';

  @override
  String get languageEn => 'English';

  @override
  String get sectionAbout => '关于';

  @override
  String get version => '版本';

  @override
  String get osUnknown => '未知';
}
