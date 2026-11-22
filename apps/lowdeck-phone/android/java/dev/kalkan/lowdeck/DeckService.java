package dev.kalkan.lowdeck;

import android.app.Activity;
import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.content.pm.ServiceInfo;
import android.os.Build;
import android.os.IBinder;

/**
 * Foreground service (ADR-0011): süreci arka planda canlı tutar; TCP bağlantısı Rust'ta.
 * Statik yardımcılar Rust'tan JNI ile çağrılır (lib.rs `notify` modülü).
 */
public class DeckService extends Service {
    private static final String CH_SERVICE = "service";
    private static final String CH_ALERTS = "alerts";
    private static final int ID_SERVICE = 1;

    /** Kanalları kurar, gerekirse bildirim iznini ister, servisi başlatır. Activity öndeyken çağrılır. */
    public static void start(final Activity a) {
        NotificationManager nm = a.getSystemService(NotificationManager.class);
        nm.createNotificationChannel(
                new NotificationChannel(CH_SERVICE, "Connection", NotificationManager.IMPORTANCE_MIN));
        NotificationChannel alerts =
                new NotificationChannel(CH_ALERTS, "PC alerts", NotificationManager.IMPORTANCE_HIGH);
        alerts.enableVibration(true);
        nm.createNotificationChannel(alerts);
        if (Build.VERSION.SDK_INT >= 33
                && a.checkSelfPermission("android.permission.POST_NOTIFICATIONS")
                        != PackageManager.PERMISSION_GRANTED) {
            a.runOnUiThread(new Runnable() {
                public void run() {
                    a.requestPermissions(new String[] {"android.permission.POST_NOTIFICATIONS"}, 1);
                }
            });
        }
        a.startForegroundService(new Intent(a, DeckService.class));
    }

    /** PC alarmı bildirimi; aynı `id` öncekinin yerine geçer. */
    public static void alert(Context c, int id, String title) {
        Notification n = builder(c, CH_ALERTS)
                .setSmallIcon(android.R.drawable.stat_sys_warning)
                .setContentTitle(title)
                .setContentText("Tap to open LowDeck")
                .setAutoCancel(true)
                .build();
        c.getSystemService(NotificationManager.class).notify(id, n);
    }

    public static void cancel(Context c, int id) {
        c.getSystemService(NotificationManager.class).cancel(id);
    }

    private static Notification.Builder builder(Context c, String channel) {
        Intent open = c.getPackageManager().getLaunchIntentForPackage(c.getPackageName());
        PendingIntent pi = PendingIntent.getActivity(
                c, 0, open, PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
        return new Notification.Builder(c, channel).setContentIntent(pi);
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        Notification n = builder(this, CH_SERVICE)
                .setSmallIcon(android.R.drawable.stat_notify_sync_noanim)
                .setContentTitle("LowDeck")
                .setContentText("Listening for PC alerts")
                .setOngoing(true)
                .build();
        startForeground(ID_SERVICE, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE);
        // Sistem süreci öldürürse yeniden başlatma: native taraf (bağlantı) Activity olmadan kurulmaz.
        return START_NOT_STICKY;
    }

    @Override
    public void onTaskRemoved(Intent rootIntent) {
        stopSelf();
    }

    @Override
    public IBinder onBind(Intent intent) {
        return null;
    }
}
