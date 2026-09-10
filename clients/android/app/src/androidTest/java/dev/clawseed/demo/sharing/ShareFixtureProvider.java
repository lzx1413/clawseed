package dev.clawseed.demo.sharing;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.content.Intent;
import android.database.MatrixCursor;
import android.graphics.Bitmap;
import android.graphics.Color;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import android.os.Bundle;
import android.provider.OpenableColumns;
import java.io.File;
import java.io.FileNotFoundException;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.RandomAccessFile;
import java.nio.charset.StandardCharsets;

/** Separate-UID fixture sender: use only framework classes, not target APK libraries. */
public class ShareFixtureProvider extends ContentProvider {
    @Override public boolean onCreate() { return true; }

    @Override public Bundle call(String method, String arg, Bundle extras) {
        if ("grant".equals(method)) {
            getContext().grantUriPermission("dev.clawseed.demo",
                Uri.parse("content://dev.clawseed.demo.test.share-fixtures/"),
                Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_PREFIX_URI_PERMISSION);
        }
        return new Bundle();
    }

    @Override public String getType(Uri uri) {
        switch (uri.getLastPathSegment()) {
            case "image": return "image/png";
            case "csv": return "text/csv";
            case "pdf": return "application/pdf";
            case "unsupported": return "application/msword";
            default: return "text/markdown";
        }
    }

    @Override public MatrixCursor query(Uri uri, String[] projection, String selection, String[] selectionArgs, String sortOrder) {
        String name;
        switch (uri.getLastPathSegment()) {
            case "image": name = "分享 图片.png"; break;
            case "csv": name = "分享 表格.csv"; break;
            case "pdf": name = "分享 文档.pdf"; break;
            case "unsupported": name = "旧文档.doc"; break;
            default: name = "分享 说明.md";
        }
        MatrixCursor cursor = new MatrixCursor(new String[]{OpenableColumns.DISPLAY_NAME});
        cursor.addRow(new Object[]{name});
        return cursor;
    }

    @Override public ParcelFileDescriptor openFile(Uri uri, String mode) throws FileNotFoundException {
        if (!"r".equals(mode)) throw new SecurityException("read only");
        String kind = uri.getLastPathSegment();
        if ("denied".equals(kind)) throw new SecurityException("Fixture permission denied");
        File file = new File(getContext().getCacheDir(), "share-fixture-" + kind);
        try {
            if ("oversized".equals(kind)) {
                try (RandomAccessFile output = new RandomAccessFile(file, "rw")) {
                    output.setLength(21L * 1024 * 1024);
                }
            } else try (FileOutputStream output = new FileOutputStream(file)) {
                if ("image".equals(kind)) {
                    Bitmap bitmap = Bitmap.createBitmap(320, 240, Bitmap.Config.ARGB_8888);
                    try {
                        bitmap.eraseColor(Color.BLUE);
                        bitmap.compress(Bitmap.CompressFormat.PNG, 100, output);
                    } finally { bitmap.recycle(); }
                } else {
                    String text = "csv".equals(kind) ? "name,value\nshared,\"SHARE-73\nsecond line\"\n"
                        : "pdf".equals(kind) ? "%PDF-1.4\nbroken fixture"
                        : "# Shared document\nVerification: SHARE-72\n";
                    output.write(text.getBytes(StandardCharsets.UTF_8));
                }
            }
        } catch (IOException error) { throw new FileNotFoundException(error.toString()); }
        return ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY);
    }

    @Override public Uri insert(Uri uri, ContentValues values) { throw new UnsupportedOperationException("read only"); }
    @Override public int delete(Uri uri, String selection, String[] selectionArgs) { return 0; }
    @Override public int update(Uri uri, ContentValues values, String selection, String[] selectionArgs) { return 0; }
}
